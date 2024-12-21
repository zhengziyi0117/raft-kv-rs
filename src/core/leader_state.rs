use std::{collections::HashMap, fmt::Display, time::Duration};

use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::{self, timeout, Instant},
};

use crate::{
    core::node_status::RaftNodeStatus, raft_proto::{AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply}, raft_server::NodeId, RAFT_APPEND_ENTRIES_INTERVAL, RAFT_COMMIT_INTERVAL, RAFT_COMMON_INTERVAL
};

use super::{node_status::RaftStateMachine, RaftCore, RaftMessage};

pub(crate) struct RaftLeaderState<'a> {
    core: &'a mut RaftCore,
    next_index: Vec<i32>,
    match_index: Vec<i32>,
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
        let next_index = vec![0; raft_node_cnt];
        let match_index = vec![-1; raft_node_cnt];
        RaftLeaderState {
            core,
            next_index,
            match_index,
        }
    }

    // 没有append_entries就是心跳，如果有append_entries就是附加日志
    async fn broadcast_append_entries(
        &mut self,
        append_entries_tx: &UnboundedSender<(NodeId, AppendEntriesReply)>,
    ) {
        let peers: Vec<NodeId> = self.core.peer2uri.keys().cloned().collect();
        for peer in peers {
            if peer == self.core.me {
                continue;
            }
            let tx = append_entries_tx.clone();
            let args = AppendEntriesArgs {
                term: self.core.current_term,
                leader_id: self.core.me,
                leader_commit: -1,
                prev_log_index: -1,
                prev_log_term: -1,
                entries: vec![],
            };
            let mut channel = match self.core.get_channel(&peer).await {
                Ok(channel) => channel,
                Err(err) => {
                    // TODO continue使用
                    // log::warn!("get append_entries channel error{}", err);
                    continue;
                }
            };
            tokio::spawn(async move {
                // TODO 可以尝试优化成spawn多个线程持续发
                match channel.append_entries(args).await {
                    Ok(reply) => {
                        let _ = tx.send((peer, reply.into_inner()));
                    }
                    Err(err) => {
                        log::warn!("send append entries to peer:{}, but get error {}", peer, err);
                    }
                };
            });
        }
    }
}

impl RaftStateMachine for RaftLeaderState<'_> {
    async fn event_loop(mut self) {
        let mut broad_append_entries_ticker =
            tokio::time::interval_at(Instant::now(), RAFT_APPEND_ENTRIES_INTERVAL);
        let mut commit_ticker = tokio::time::interval(RAFT_COMMIT_INTERVAL);
        let (append_entries_tx, mut append_entries_rx) =
            unbounded_channel::<(NodeId, AppendEntriesReply)>();

        loop {
            if self.core.status != RaftNodeStatus::Leader {
                return;
            }

            select! {
                msg = self.core.msg_rx.recv() => {
                    match msg {
                        Some(RaftMessage::AppendEntriesRequest(args, tx)) => {
                            let reply = self.core.handle_append_entries(args).await;
                            tx.send(reply).unwrap();
                        }
                        Some(RaftMessage::RequestVoteRequest(args, tx)) => {
                            let reply = self.core.handle_request_vote(args).await;
                            tx.send(reply).unwrap();
                        }
                        Some(RaftMessage::Shutdown) | None => {
                            self.core.status = RaftNodeStatus::Shutdown;
                            return
                        }
                    }
                }
                msg = append_entries_rx.recv() => {
                    match msg {
                        Some((node_id,reply)) => {

                        },
                        None => {
                            // TODO continue使用
                            log::warn!("get channel error");
                        }
                    }
                }
                _ = broad_append_entries_ticker.tick() => {
                    log::trace!("{} leader send append_entries",self);
                    self.broadcast_append_entries(&append_entries_tx).await;
                }
                _ = commit_ticker.tick() => {
                    // TODO 日志再写
                }
            }
        }
    }
}
