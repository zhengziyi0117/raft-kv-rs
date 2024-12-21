use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use clap::Parser;
use log::info;
use raft_kv_rs::{raft_proto::raft_service_server::RaftServiceServer, raft_server::RaftServer};
use tokio::signal;
use tonic::transport::Server;

#[derive(Parser)]
struct ServerOpts {
    #[arg(
        help = "bind address",
        default_value = "0.0.0.0:8080",
        long = "bind-addr"
    )]
    bind_addr: String,
    #[arg(help = "peers", long = "peers")]
    peers: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let opts = ServerOpts::parse();
    // env_logger::init();
    // let bind_addr = SocketAddr::from_str(&opts.bind_addr).expect("server bind address error");
    // let mut peers = vec![];
    // for peer in opts.peers {
    //     let peer_addr = SocketAddr::from_str(&peer).expect("peers address error");
    //     peers.push(peer_addr);
    // }

    // let server = RaftServer::new(1, HashMap::new(), signal::ctrl_c());
    // Server::builder()
    //     .add_service(RaftServiceServer::new(server))
    //     .serve(bind_addr)
    //     .await?;
    // info!("Starting raft server {}", bind_addr);
    Ok(())
}
