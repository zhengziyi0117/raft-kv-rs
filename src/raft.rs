use crate::{
    core::{RaftCore, RaftMessage}, fsm::{self, FinishedStateMachine}, grpc_server::RaftGrpcServer, http_server::RaftHttpServer, raft_proto::Entry, NodeId
};
use std::{collections::HashMap, future::Future, net::SocketAddr};
use tokio::sync::{broadcast, mpsc::unbounded_channel};

pub async fn spawn(
    me: NodeId,
    peers: HashMap<NodeId, SocketAddr>,
    http_addr: SocketAddr,
    fsm: impl FinishedStateMachine + Send + 'static,
    shutdown: impl Future,
) {
    let (tx, rx) = unbounded_channel::<RaftMessage>();
    let grpc_addr = peers.get(&me).unwrap().clone();
    let (shutdown_tx, _rx) = broadcast::channel(1);
    let (apply_tx, apply_rx) = unbounded_channel::<Entry>();

    let core_handle = RaftCore::spawn(me, peers, rx, shutdown_tx.subscribe(),apply_tx);
    let grpc_handle =
        tokio::spawn(RaftGrpcServer::new(tx.clone(), grpc_addr, shutdown_tx.subscribe()).start());
    let http_handle =
        tokio::spawn(RaftHttpServer::new(tx.clone(), http_addr, shutdown_tx.subscribe()).start());
    let apply_handle = tokio::spawn(fsm::apply_logs(fsm, apply_rx, shutdown_tx.subscribe()));
    
    shutdown.await;
    let _ = shutdown_tx.send(());
    let _ = core_handle.await;
    let _ = grpc_handle.await;
    let _ = http_handle.await;
    let _ = apply_handle.await;
}
