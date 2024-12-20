use std::{net::SocketAddr, str::FromStr};

use raft_kv_rs::raft_proto::{
    raft_service_server::{RaftService, RaftServiceServer},
    AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply,
};
use tonic::{transport::Server, Request, Response, Status};

pub struct RaftTestServer {}

#[tonic::async_trait]
impl RaftService for RaftTestServer {
    async fn request_vote(
        &self,
        request: Request<RequestVoteArgs>,
    ) -> Result<Response<RequestVoteReply>, Status> {
        let reply = RequestVoteReply {
            term: 0,
            vote_granted: true,
        };
        Ok(Response::new(reply))
    }

    async fn append_entries(
        &self,
        request: Request<AppendEntriesArgs>,
    ) -> Result<Response<AppendEntriesReply>, Status> {
        let reply = AppendEntriesReply {
            term: 0,
            success: true,
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let bind_addr = SocketAddr::from_str("0.0.0.0:8080").expect("bind error");
    let server = RaftTestServer {};
    log::info!("-1Starting raft server {}", bind_addr);
    Server::builder()
        .add_service(RaftServiceServer::new(server))
        .serve(bind_addr)
        .await?;
    log::info!("Starting raft server {}", bind_addr);
    Ok(())
}
