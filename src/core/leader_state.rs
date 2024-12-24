use std::{collections::HashMap, fmt::Display};

use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::Instant,
};

use crate::{
    core::{RaftGrpcHandler, RaftHttpHandle, RaftNodeStatus},
    fsm::apply_logs,
    raft_proto::{AppendEntriesArgs, AppendEntriesReply, Entry},
    NodeId, RAFT_APPEND_ENTRIES_INTERVAL, RAFT_COMMIT_INTERVAL,
};

use super::{RaftCore, RaftMessage, RaftStateEventLoop};

pub(crate) struct RaftLeaderState<'a> {
    core: &'a mut RaftCore,
    // 这俩里面不会带有me
    next_index: HashMap<NodeId, u32>,
    match_index: HashMap<NodeId, u32>,
}

impl Display for RaftLeaderState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} next_index:{:?},match_index={:?}",
            self.core, self.next_index, self.match_index
        )
    }
}

impl<'a> RaftLeaderState<'a> {
    pub(super) fn new(core: &'a mut RaftCore) -> Self {
        let raft_node_cnt = core.peer2uri.len();
        let mut next_index = HashMap::with_capacity(raft_node_cnt);
        let mut match_index = HashMap::with_capacity(raft_node_cnt);
        for peer in core.peer2uri.keys() {
            let peer = *peer;
            if core.me == peer {
                continue;
            }
            next_index.insert(peer, core.logs.len() as u32);
            match_index.insert(peer, 0);
        }
        RaftLeaderState {
            core,
            next_index,
            match_index,
        }
    }

    // 没有append_entries就是心跳，如果有append_entries就是附加日志
    async fn broadcast_append_entries(
        &mut self,
        append_entries_tx: &UnboundedSender<(NodeId, usize, AppendEntriesReply)>,
    ) {
        log::trace!("{} leader send append_entries", self);
        let peers: Vec<NodeId> = self.core.peer2uri.keys().cloned().collect();
        for peer in peers {
            if peer == self.core.me {
                continue;
            }
            let tx = append_entries_tx.clone();
            let (prev_log_term, prev_log_index, logs) = self.get_append_logs(&peer);
            let args = AppendEntriesArgs {
                term: self.core.current_term,
                leader_id: self.core.me,
                leader_commit: self.core.commit_index,
                prev_log_index: prev_log_index,
                prev_log_term: prev_log_term,
                entries: logs,
            };
            let mut channel = match self.core.get_channel(&peer).await {
                Ok(channel) => channel,
                Err(err) => {
                    log::warn!("get append_entries channel error:{}", err);
                    continue;
                }
            };
            tokio::spawn(async move {
                // TODO 可以尝试优化成spawn多个线程持续发
                let send_log_cnt = args.entries.len();
                match channel.append_entries(args).await {
                    Ok(reply) => {
                        let _ = tx.send((peer, send_log_cnt, reply.into_inner()));
                    }
                    Err(err) => {
                        log::warn!(
                            "send append entries to peer:{}, but get error:{}",
                            peer,
                            err
                        );
                    }
                };
            });
        }
    }

    // 返回值 last_log_term last_log_index logs
    fn get_append_logs(&self, peer_index: &NodeId) -> (u32, u32, Vec<Entry>) {
        let next_index = *self.next_index.get(&peer_index).unwrap();
        let (last_log_term, last_log_index) = self.core.get_last_log_term_and_index();
        if next_index > last_log_index {
            (last_log_term, last_log_index, vec![])
        } else {
            let logs = self.core.logs[next_index as usize..].to_vec();
            let prev_log_index = next_index.checked_sub(1).unwrap();
            let prev_log_term = self.core.logs[prev_log_index as usize].term;
            (prev_log_term, prev_log_index, logs)
        }
    }

    fn try_commit_and_apply(&mut self) {
        let (_, last_log_index) = self.core.get_last_log_term_and_index();
        for commit_index in self.core.commit_index+1..=last_log_index {
            // 只commit当前任期的日志
            if self.core.logs[commit_index as usize].term != self.core.current_term {
                continue;
            }
            let mut match_cnt = 1;
            for match_index in self.match_index.values() {
                if *match_index >= commit_index {
                    match_cnt += 1;
                }
            }
            if (2 * match_cnt) - 1 < self.core.peer2uri.len() {
                continue;
            }
            // 大于半数以上
            self.core.commit_index = commit_index;
            for apply_index in self.core.last_applied+1..=commit_index {
                let entry = &self.core.logs[apply_index as usize];
                self.core.apply_tx.send(entry.clone()).unwrap();
                self.core.last_applied = apply_index;
            }
            log::info!("{} leader apply some msg", self);
        }
    }
}

impl RaftStateEventLoop for RaftLeaderState<'_> {
    async fn event_loop(mut self) {
        let mut broad_append_entries_ticker =
            tokio::time::interval_at(Instant::now(), RAFT_APPEND_ENTRIES_INTERVAL);
        let mut commit_ticker = tokio::time::interval(RAFT_COMMIT_INTERVAL);
        // channel传输值 node_id 日志发送的数量 reply
        let (append_entries_tx, mut append_entries_rx) =
            unbounded_channel::<(NodeId, usize, AppendEntriesReply)>();

        loop {
            if self.core.status != RaftNodeStatus::Leader {
                return;
            }

            select! {
                msg = self.core.msg_rx.recv() => {
                    match msg {
                        // grpc api
                        Some(RaftMessage::AppendEntriesRequest(args, tx)) => {
                            let reply = self.core.handle_append_entries(args);
                            tx.send(reply).unwrap();
                        }
                        Some(RaftMessage::RequestVoteRequest(args, tx)) => {
                            // 可能状态变成follower，但是这里select结束后会有判断leader是否变成follower
                            let reply = self.core.handle_request_vote(args);
                            tx.send(reply).unwrap();
                        }
                        // http api
                        Some(RaftMessage::GetStatusRequest(tx)) => {
                            let reply = self.core.handle_get_status();
                            tx.send(reply).unwrap();
                        }
                        Some(RaftMessage::AddLogRequest(args,tx)) => {
                            let reply = self.core.handle_add_log(args);
                            tx.send(reply).unwrap();
                        }
                        Some(RaftMessage::ListLogsRequest(tx)) => {
                            let reply = self.core.handle_list_logs();
                            tx.send(reply).unwrap();
                        }
                        None => {
                            self.core.status = RaftNodeStatus::Shutdown;
                            return
                        }
                    }
                }
                msg = append_entries_rx.recv() => {
                    match msg {
                        Some((node_id,send_log_cnt,reply)) => {
                            // TODO 用于日志确认
                            // if reply.term > self.core.current_term {
                            //     self.core.current_term = reply.term;
                            //     self.core.status = RaftNodeStatus::Follower;
                            //     return
                            // }
                            if reply.success && reply.term == self.core.current_term && send_log_cnt > 0 {
                                let next_index = self.next_index.get_mut(&node_id).unwrap();
                                let match_index = self.match_index.get_mut(&node_id).unwrap();
                                *next_index = *next_index + send_log_cnt as u32;
                                // TODO 换成prev_log_index + send_log_cnt
                                *match_index = *next_index - 1;
                            }
                        },
                        None => {
                            log::error!("{} append_entries_rx recv None",self);
                            return
                        }
                    }
                }
                _ = broad_append_entries_ticker.tick() => {
                    self.broadcast_append_entries(&append_entries_tx).await;
                }
                _ = commit_ticker.tick() => {
                    // TODO 日志commit
                    self.try_commit_and_apply();
                }
                _ = self.core.shutdown.recv() => {
                    self.core.status = RaftNodeStatus::Shutdown;
                    return
                }
            }
        }
    }
}
