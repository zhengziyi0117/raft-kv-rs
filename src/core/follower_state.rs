use std::fmt::Display;

use tokio::{select, time::Instant};

use crate::{
    core::{RaftGrpcHandler, RaftHttpHandle, RaftNodeStatus}, RAFT_COMMIT_INTERVAL, RAFT_COMMON_INTERVAL
};

use super::{RaftCore, RaftMessage, RaftStateEventLoop};

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

    fn commit_and_apply(&mut self) {
        for apply_index in self.core.last_applied+1..=self.core.commit_index {
            let entry = &self.core.logs[apply_index as usize];
            self.core.apply_tx.send(entry.clone()).unwrap();
            self.core.last_applied = apply_index;
        }
    }
}

impl RaftStateEventLoop for RaftFollowerState<'_> {
    async fn event_loop(mut self) {
        let mut ticker = tokio::time::interval_at(Instant::now(), RAFT_COMMON_INTERVAL);
        let mut commit_ticker = tokio::time::interval(RAFT_COMMIT_INTERVAL);
        loop {
            if self.core.status != RaftNodeStatus::Follower {
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
                _ = ticker.tick() => {
                    // 如果一段时间没有收到响应消息，就会走到这里，检查状态
                    if self.core.is_elect_timeout() {
                        // follower这里timeout直接转成candidate，进入candidate的eventloop中做term递增以及开启投票等操作
                        self.core.status = RaftNodeStatus::Candidate;
                        // self.core.current_term += 1;
                        // self.core.last_update_time = Instant::now();
                        // self.core.next_elect_timeout = RaftCore::gen_next_elect_timeout();
                        // self.core.voted_for = Some(self.core.me);
                        // log::info!("{} follower elect timeout", self);
                        return
                    }
                }
                _ = commit_ticker.tick() => {
                    self.commit_and_apply();
                }
                _ = self.core.shutdown.recv() => {
                    self.core.status = RaftNodeStatus::Shutdown;
                    return
                }
            }
        }
    }
}
