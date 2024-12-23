use crate::{
    core::{RaftCore, RaftMessage},
    grpc_server::{self, RaftGrpcServer},
    http_server::RaftHttpServer,
    raft_proto::{
        raft_service_server::{RaftService, RaftServiceServer},
        AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply,
    },
    NodeId,
};
use std::{collections::HashMap, future::Future, net::SocketAddr, sync::Arc};
use tokio::{
    sync::{
        broadcast,
        mpsc::{unbounded_channel, UnboundedSender},
        oneshot,
    },
    task::JoinHandle,
};
use tonic::{transport::Server, Request, Response, Status};

pub async fn spawn(
    me: NodeId,
    peers: HashMap<NodeId, SocketAddr>,
    http_addr: SocketAddr,
    shutdown: impl Future,
) {
    let (tx, rx) = unbounded_channel::<RaftMessage>();
    let grpc_addr = peers.get(&me).unwrap().clone();
    let (shutdown_tx, _rx) = broadcast::channel(1);

    let core_handle = RaftCore::spawn(me, peers, rx);
    let grpc_handle =
        tokio::spawn(RaftGrpcServer::new(tx.clone(), grpc_addr, shutdown_tx.subscribe()).start());
    let http_handle =
        tokio::spawn(RaftHttpServer::new(tx.clone(), http_addr, shutdown_tx.subscribe()).start());

    shutdown.await;
    let _ = shutdown_tx.send(());
    let _ = core_handle.await;
    let _ = grpc_handle.await;
    let _ = http_handle.await;
}
