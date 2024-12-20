use std::time::Duration;

pub mod raft_server;
pub mod raft_proto {
    tonic::include_proto!("raft"); // The string specified here must match the proto package name
}

mod core;

const RAFT_APPEND_ENTRIES_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMIT_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMON_INTERVAL: Duration = Duration::from_millis(50);
