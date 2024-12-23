use std::{
    collections::HashMap, fs::OpenOptions, future::Future, net::SocketAddr, str::FromStr,
};

use raft_kv_rs::{raft, NodeId};
use tokio::signal::ctrl_c;

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

async fn start_raft(me: NodeId, peers: HashMap<NodeId, SocketAddr>, http_addr: SocketAddr) {
    raft::spawn(me, peers.clone(), http_addr, ctrl_c()).await;
}

#[tokio::main]
async fn main() {
    init_log();
    // 创建三个节点
    // 其中807x为toxiproxy代理地址，809x为raft节点地址
    // 606x为http地址

    let http_addr0 = SocketAddr::from_str("0.0.0.0:6060").unwrap();
    let mut peers0 = HashMap::<NodeId, SocketAddr>::new();
    peers0.insert(0, SocketAddr::from_str("0.0.0.0:8090").unwrap());
    peers0.insert(1, SocketAddr::from_str("0.0.0.0:8071").unwrap());
    peers0.insert(2, SocketAddr::from_str("0.0.0.0:8072").unwrap());
    let handle0 = tokio::spawn(start_raft(0, peers0, http_addr0));

    let http_addr1 = SocketAddr::from_str("0.0.0.0:6061").unwrap();
    let mut peers1 = HashMap::<NodeId, SocketAddr>::new();
    peers1.insert(0, SocketAddr::from_str("0.0.0.0:8070").unwrap());
    peers1.insert(1, SocketAddr::from_str("0.0.0.0:8091").unwrap());
    peers1.insert(2, SocketAddr::from_str("0.0.0.0:8072").unwrap());
    let handle1 = tokio::spawn(start_raft(1, peers1, http_addr1));

    let http_addr2 = SocketAddr::from_str("0.0.0.0:6062").unwrap();
    let mut peers2 = HashMap::<NodeId, SocketAddr>::new();
    peers2.insert(0, SocketAddr::from_str("0.0.0.0:8070").unwrap());
    peers2.insert(1, SocketAddr::from_str("0.0.0.0:8071").unwrap());
    peers2.insert(2, SocketAddr::from_str("0.0.0.0:8092").unwrap());
    let handle2 = tokio::spawn(start_raft(2, peers2, http_addr2));

    let handles = vec![handle0, handle1, handle2];
    for handle in handles {
        let _ = handle.await;
    }
}
