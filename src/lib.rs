use std::time::Duration;

pub mod raft;
pub mod http_server;
pub mod grpc_server;

pub mod raft_proto {
    tonic::include_proto!("raft"); // The string specified here must match the proto package name
}

mod core;

pub type NodeId = i32;

const RAFT_APPEND_ENTRIES_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMIT_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMON_INTERVAL: Duration = Duration::from_millis(50);
