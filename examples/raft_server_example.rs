use std::{collections::HashMap, fs::OpenOptions, net::SocketAddr, str::FromStr};

use raft_kv_rs::{fsm::FinishedStateMachine, raft, NodeId};
use tokio::signal::ctrl_c;

#[derive(Default)]
pub struct TestFSM {

}

impl FinishedStateMachine for TestFSM {
    fn apply(&mut self, entry: raft_kv_rs::raft_proto::Entry) {
        log::info!("apply entry: {}", entry);
    }
}

fn init_log() {
    let _file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("test.log")
        .unwrap();
    let mut builder = env_logger::Builder::from_default_env();
    builder
        // .target(Target::Pipe(Box::new(file))) // 日志写入文件
        .filter(Some("raft_kv_rs"),log::LevelFilter::Trace) // 设置日志级别
        .format_timestamp_millis()
        .init();
}

#[tokio::main]
async fn main() {
    init_log();
    let http_addr0 = SocketAddr::from_str("0.0.0.0:6060").unwrap();
    let mut peers0 = HashMap::<NodeId, SocketAddr>::new();
    peers0.insert(0, SocketAddr::from_str("0.0.0.0:8090").unwrap());
    raft::spawn(0, peers0.clone(), http_addr0,TestFSM::default(), ctrl_c()).await;
}