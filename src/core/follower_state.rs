use std::fmt::Display;

use tokio::{select, time::Instant};

use crate::{
    core::{RaftGrpcHandler, RaftHttpHandle, RaftNodeStatus},
    RAFT_COMMON_INTERVAL,
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
}

impl RaftStateEventLoop for RaftFollowerState<'_> {
    async fn event_loop(self) {
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
                        Some(RaftMessage::GetStatusRequest(tx)) => {
                            let reply = self.core.handle_get_status();
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
                _ = self.core.shutdown.recv() => {
                    self.core.status = RaftNodeStatus::Shutdown;
                    return
                }
            }
        }
    }
}
