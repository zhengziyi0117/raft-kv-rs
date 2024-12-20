use std::{collections::HashMap, fmt::Display, time::Duration};

use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::{self, timeout, Instant},
};
use tonic::IntoRequest;

use crate::{
    raft_proto::{AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply},
    raft_server::NodeId,
    RAFT_APPEND_ENTRIES_INTERVAL, RAFT_COMMIT_INTERVAL, RAFT_COMMON_INTERVAL,
};

use super::{RaftCore, RaftMessage};

#[derive(Debug, PartialEq)]
pub enum RaftNodeStatus {
    Leader,
    Candidate,
    Follower,
    Shutdown,
}

pub trait RaftStateMachine {
    async fn event_loop(self);
}

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
                let t = args.clone();
                match channel.append_entries(args).await {
                    Ok(reply) => {
                        let _ = tx.send((peer, reply.into_inner()));
                    }
                    Err(err) => {
                        log::warn!("{:?} send append entries error {}", t, err);
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
                        Some(RaftMessage::Shutdown) => {
                            self.core.status = RaftNodeStatus::Shutdown;
                        }
                        None => {
                            log::error!("leader receive none");
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
                    log::info!("{} leader send append_entries",self);
                    self.broadcast_append_entries(&append_entries_tx).await;
                }
                _ = commit_ticker.tick() => {
                    // TODO 日志再写
                }
            }
        }
    }
}

pub(crate) struct RaftCandidateState<'a> {
    core: &'a mut RaftCore,
    votes: HashMap<NodeId, bool>,
}

impl Display for RaftCandidateState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RaftCandidate({},votes:{:?})", self.core, self.votes)
    }
}

impl<'a> RaftCandidateState<'a> {
    pub(crate) fn new(core: &'a mut RaftCore) -> Self {
        let votes = HashMap::new();
        Self { core, votes }
    }

    // 广播投票
    async fn broadcast_request_vote(
        &mut self,
        request_vote_tx: &UnboundedSender<(NodeId, RequestVoteReply)>,
    ) {
        let peers: Vec<NodeId> = self.core.peer2uri.keys().cloned().collect();
        for peer in peers {
            if peer == self.core.me || self.votes.contains_key(&peer) {
                continue;
            }
            let tx = request_vote_tx.clone();
            let args = RequestVoteArgs {
                term: self.core.current_term,
                candidate_id: self.core.me,
                last_log_index: -1,
                last_log_term: -1,
            };
            let mut channel = match self.core.get_channel(&peer).await {
                Ok(channel) => channel,
                Err(_) => continue,
            };
            // TODO 改成常量的duration
            tokio::spawn(timeout(Duration::from_millis(500), async move {
                loop {
                    log::info!("send request_vote to peer:{} args:{:?}", peer, args);
                    match channel.request_vote(args).await {
                        Ok(reply) => {
                            let _ = tx.send((peer, reply.into_inner()));
                            return;
                        }
                        Err(err) => {
                            log::warn!("send request vote error {}", err);
                        }
                    };
                    // 失败了就暂停一下
                    time::sleep(Duration::from_millis(50)).await;
                }
            }));
        }
    }

    fn can_to_leader(&self) -> bool {
        let all = self.core.peer2uri.len();
        let mut votes_cnt = self.votes.values().filter(|&&v| v).count();
        // 加上自己
        votes_cnt += 1;
        // 大于半数即可
        (2 * votes_cnt) - 1 >= all
    }
}

impl RaftStateMachine for RaftCandidateState<'_> {
    async fn event_loop(mut self) {
        let mut ticker = tokio::time::interval_at(Instant::now(), RAFT_COMMON_INTERVAL);
        let (request_vote_tx, mut request_vote_rx) =
            unbounded_channel::<(NodeId, RequestVoteReply)>();
        loop {
            if self.core.status != RaftNodeStatus::Candidate {
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
                        Some(RaftMessage::Shutdown) => {
                            self.core.status = RaftNodeStatus::Shutdown;
                        }
                        None => {
                            log::error!("{} candidate receive msg from channel none!", self);
                        }
                    }
                }
                _ = ticker.tick() => {
                    // 如果一段时间没有收到响应消息，就会走到这里，检查状态
                    if self.core.is_elect_timeout() {
                        // 转为 candidate 本轮 term 没选举成功 下一回合
                        log::info!("{} candidate elect timeout and start a new term", self);
                        self.core.current_term += 1;
                        self.core.status = RaftNodeStatus::Candidate;
                        self.core.last_update_time = Instant::now();
                        self.core.next_elect_timeout = RaftCore::gen_next_elect_timeout();
                        self.core.voted_for = Some(self.core.me);
                        self.votes.clear();
                        // 开启新的一轮投票
                        // 1. 针对每个node spawn一个协程去 request_vote
                        // 2. 每个协程有超时时间，如果超时自动关闭
                        // 3. 每次新开启一轮投票都会spawn相应的协程
                        // 4. 使用channel来通信，在这个主要的select下处理
                        // 5. 定时校验投票是否足够变成 leader
                        self.broadcast_request_vote(&request_vote_tx).await;
                    }
                }
                msg = request_vote_rx.recv() => {
                    match msg {
                        Some((node_id,reply)) => {
                            // TODO 日志确认
                            if self.core.current_term == reply.term {
                                self.votes.insert(node_id,reply.vote_granted);
                                if self.can_to_leader() {
                                    self.core.status = RaftNodeStatus::Leader;
                                }
                            }
                        },
                        None => todo!(),
                    }
                }
            }
        }
    }
}

pub(crate) struct RaftFollowerState<'a> {
    core: &'a mut RaftCore,
}

impl Display for RaftFollowerState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.core.fmt(f)
    }
}

impl<'a> RaftFollowerState<'a> {
    pub(crate) fn new(core: &'a mut RaftCore) -> Self {
        Self { core }
    }
}

impl RaftStateMachine for RaftFollowerState<'_> {
    async fn event_loop(mut self) {
        let mut ticker = tokio::time::interval_at(Instant::now(), RAFT_COMMON_INTERVAL);
        loop {
            if self.core.status != RaftNodeStatus::Follower {
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
                        Some(RaftMessage::Shutdown) => {
                            self.core.status = RaftNodeStatus::Shutdown;
                        }
                        None => {
                            log::error!("{} follow receive msg from channel none!", self);
                        }
                    }
                }
                _ = ticker.tick() => {
                    // 如果一段时间没有收到响应消息，就会走到这里，检查状态
                    if self.core.is_elect_timeout() {
                        // 转为 candidate 直接 return 结束 follower的 loop
                        self.core.current_term += 1;
                        self.core.last_update_time = Instant::now();
                        self.core.status = RaftNodeStatus::Candidate;
                        self.core.next_elect_timeout = RaftCore::gen_next_elect_timeout();
                        self.core.voted_for = Some(self.core.me);
                        log::info!("{} follower elect timeout", self);
                    }
                }
            }
        }
    }
}
