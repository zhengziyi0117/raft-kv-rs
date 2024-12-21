use std::{collections::HashMap, fs::OpenOptions, net::SocketAddr, str::FromStr};

use env_logger::Target;
use raft_kv_rs::raft_server::{NodeId, RaftServer};
use tokio::signal::ctrl_c;

async fn start_raft(me: NodeId, peers: HashMap<NodeId, SocketAddr>) {
    RaftServer::new(me, peers.clone()).start(ctrl_c()).await.unwrap();
}

fn init_log() {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("test.log")
        .unwrap();
    let mut builder = env_logger::Builder::from_default_env();
    builder
        // .target(Target::Pipe(Box::new(file))) // 日志写入文件
        .filter_level(log::LevelFilter::Trace) // 设置日志级别
        .format_timestamp_millis()
        .init();
}

#[tokio::main]
async fn main() {
    init_log();
    let mut peers = HashMap::<NodeId, SocketAddr>::new();
    peers.insert(0, SocketAddr::from_str("0.0.0.0:8090").unwrap());
    peers.insert(1, SocketAddr::from_str("0.0.0.0:8091").unwrap());
    peers.insert(2, SocketAddr::from_str("0.0.0.0:8092").unwrap());
    let handle1 = tokio::spawn(start_raft(0, peers.clone()));
    let handle2 = tokio::spawn(start_raft(1, peers.clone()));
    let handle3 = tokio::spawn(start_raft(2, peers.clone()));
    let vs = vec![handle1, handle2, handle3];
    for v in vs {
        let _ = v.await;
    }
}
