use crate::{
    core::{RaftCore, RaftMessage},
    raft_proto::{
        raft_service_server::{RaftService, RaftServiceServer},
        AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply,
    },
    RAFT_APPEND_ENTRIES_INTERVAL,
};
use std::{collections::HashMap, future::Future, net::SocketAddr, sync::Arc};
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::JoinHandle,
};
use tonic::{
    server,
    transport::{Error, Server},
    Request, Response, Status,
};

pub type NodeId = i32;

struct RaftInner {
    tx_api: UnboundedSender<RaftMessage>,
    raft_core_handle: JoinHandle<()>,
    bind_addr: SocketAddr,
}

pub struct RaftServer {
    inner: Arc<RaftInner>,
}

impl RaftServer {
    pub fn new(
        me: NodeId,
        peers: HashMap<NodeId, SocketAddr>,
        shutdown: impl Future + Send + 'static,
    ) -> Self {
        let (tx, rx) = unbounded_channel::<RaftMessage>();
        let bind_addr = peers.get(&me).unwrap().clone();
        let raft_core_handle = RaftCore::spawn(me, peers, rx);
        tokio::spawn(RaftServer::check_shutdown(shutdown, tx.clone()));

        let inner = RaftInner {
            tx_api: tx,
            raft_core_handle,
            bind_addr,
        };
        let this = RaftServer {
            inner: Arc::new(inner),
        };

        this
    }

    pub async fn start(self) -> Result<(), tonic::transport::Error> {
        let bind_addr = self.inner.bind_addr;
        // TODO server_with_shutdown
        log::info!("raft server start! bind:{:?}", bind_addr);
        Server::builder()
            .add_service(RaftServiceServer::new(self))
            .serve(bind_addr)
            .await?;
        Ok(())
    }

    async fn check_shutdown(shutdown: impl Future + 'static, tx: UnboundedSender<RaftMessage>) {
        shutdown.await;
        log::info!("receiver ctrl_c");
        let _ = tx.send(RaftMessage::Shutdown);
    }
}

#[tonic::async_trait]
impl RaftService for RaftServer {
    async fn request_vote(
        &self,
        request: Request<RequestVoteArgs>,
    ) -> Result<Response<RequestVoteReply>, Status> {
        let (tx, rx) = oneshot::channel::<RequestVoteReply>();
        self.inner
            .tx_api
            .send(RaftMessage::RequestVoteRequest(
                request.get_ref().clone(),
                tx,
            ))
            .map_err(|err| Status::aborted("server shutdown"))?;
        match rx.await {
            Ok(reply) => Ok(Response::new(reply)),
            Err(err) => {
                log::error!("receive err:{}", err);
                Err(Status::aborted("server shutdown"))
            }
        }
    }

    async fn append_entries(
        &self,
        request: Request<AppendEntriesArgs>,
    ) -> Result<Response<AppendEntriesReply>, Status> {
        let (tx, rx) = oneshot::channel::<AppendEntriesReply>();
        self.inner
            .tx_api
            .send(RaftMessage::AppendEntriesRequest(
                request.get_ref().clone(),
                tx,
            ))
            .map_err(|err| Status::aborted("server shutdown"))?;
        match rx.await {
            Ok(reply) => Ok(Response::new(reply)),
            Err(err) => {
                log::error!("receive err:{}", err);
                Err(Status::aborted("server shutdown"))
            }
        }
    }
}
