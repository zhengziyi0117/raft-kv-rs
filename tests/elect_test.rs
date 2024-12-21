use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use raft_kv_rs::raft_server::{NodeId, RaftServer};
use tokio::signal::ctrl_c;

#[tokio::test]
async fn test_elect() {
    let mut peers = HashMap::<NodeId, SocketAddr>::new();
    peers.insert(0, SocketAddr::from_str("0.0.0.0:8080").unwrap());
    peers.insert(1, SocketAddr::from_str("0.0.0.0:8081").unwrap());
    peers.insert(2, SocketAddr::from_str("0.0.0.0:8082").unwrap());
    // RaftServer::new(0, peers.clone(), ctrl_c()).start().await;
    // RaftServer::new(0, peers.clone(), ctrl_c()).start().await;
    // RaftServer::new(0, peers.clone(), ctrl_c()).start().await;
}

#[tokio::test]
async fn test_drop_elect() {}
