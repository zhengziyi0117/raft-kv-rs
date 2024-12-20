use std::future::Future;
use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use chrono::Local;
use env_logger::Builder;
use raft_kv_rs::raft_server::{NodeId, RaftServer};
use tokio::signal::ctrl_c;
use tokio::{
    select,
    sync::mpsc::unbounded_channel,
    time::{interval, sleep},
};

async fn start_raft(me: NodeId, peers: HashMap<NodeId, SocketAddr>) {
    RaftServer::new(me, peers.clone(), ctrl_c()).start().await;
}

fn init_log() {
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .filter_level(log::LevelFilter::Trace) // 设置日志级别
        .format_timestamp_millis()
        .init();
}

#[tokio::main]
async fn main() {
    init_log();
    let mut peers = HashMap::<NodeId, SocketAddr>::new();
    peers.insert(0, SocketAddr::from_str("0.0.0.0:8080").unwrap());
    peers.insert(1, SocketAddr::from_str("0.0.0.0:8081").unwrap());
    peers.insert(2, SocketAddr::from_str("0.0.0.0:8082").unwrap());
    let handle1 = tokio::spawn(start_raft(0, peers.clone()));
    let handle2 = tokio::spawn(start_raft(1, peers.clone()));
    let handle3 = tokio::spawn(start_raft(2, peers.clone()));
    let vs = vec![handle1, handle2, handle3];
    for v in vs {
        let _ = v.await;
    }
}
