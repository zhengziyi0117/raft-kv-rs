use std::{collections::HashMap, fmt::Display, time::Duration};

use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::{self, timeout, Instant},
};

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