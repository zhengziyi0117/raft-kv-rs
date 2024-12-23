use std::net::SocketAddr;

use tokio::sync::{broadcast::Receiver, mpsc::UnboundedSender, oneshot};
use tonic::{transport::Server, Request, Response, Status};

use crate::{
    core::RaftMessage,
    raft_proto::{
        raft_service_server::{RaftService, RaftServiceServer},
        AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply,
    },
};

pub struct RaftGrpcServer {
    tx_api: UnboundedSender<RaftMessage>,
    bind_addr: SocketAddr,
    shutdown: Receiver<()>,
}

impl RaftGrpcServer {
    pub fn new(
        tx_api: UnboundedSender<RaftMessage>,
        bind_addr: SocketAddr,
        shutdown: Receiver<()>,
    ) -> Self {
        Self {
            tx_api,
            bind_addr,
            shutdown,
        }
    }

    pub async fn start(self) -> Result<(), tonic::transport::Error> {
        log::info!("raft grpc server start {:?}", self.bind_addr);
        let bind_addr = self.bind_addr;
        let mut shutdown = self.shutdown.resubscribe();
        Server::builder()
            .add_service(RaftServiceServer::new(self))
            .serve_with_shutdown(bind_addr, async move {
                shutdown.recv().await.ok();
            })
            .await?;
        Ok(())
    }
}

#[tonic::async_trait]
impl RaftService for RaftGrpcServer {
    async fn request_vote(
        &self,
        request: Request<RequestVoteArgs>,
    ) -> Result<Response<RequestVoteReply>, Status> {
        let (tx, rx) = oneshot::channel::<RequestVoteReply>();
        self.tx_api
            .send(RaftMessage::RequestVoteRequest(
                request.get_ref().clone(),
                tx,
            ))
            .map_err(|_| Status::aborted("server shutdown"))?;
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
        self.tx_api
            .send(RaftMessage::AppendEntriesRequest(
                request.get_ref().clone(),
                tx,
            ))
            .map_err(|_| Status::aborted("server shutdown"))?;
        match rx.await {
            Ok(reply) => Ok(Response::new(reply)),
            Err(err) => {
                log::error!("receive err:{}", err);
                Err(Status::aborted("server shutdown"))
            }
        }
    }
}
