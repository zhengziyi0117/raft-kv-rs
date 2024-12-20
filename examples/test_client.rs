use std::time::{self, Duration};

use raft_kv_rs::raft_proto::{
    raft_service_client::RaftServiceClient, AppendEntriesArgs, RequestVoteArgs,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tonic::transport::{Channel, Uri};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let uri = Uri::builder()
        .scheme("http")
        .authority("127.0.0.1:8080")
        .path_and_query("")
        .build()
        .unwrap();
    let client = RaftServiceClient::connect(uri).await.unwrap();
    let mut rx = request_batch(client).await;
    while let Some(time) = rx.recv().await {
        println!("{:?}", time);
    }

    Ok(())
}

async fn request_batch(mut client: RaftServiceClient<Channel>) -> UnboundedReceiver<Duration> {
    let (tx, rx) = unbounded_channel();
    for i in 0..10 {
        let clone_tx = tx.clone();
        let mut channel = client.clone();
        tokio::spawn(async move {
            loop {
                let pre = time::Instant::now();
                let request = tonic::Request::new(AppendEntriesArgs {
                    term: -1,
                    leader_id: -1,
                    prev_log_index: -1,
                    prev_log_term: -1,
                    entries: vec![],
                    leader_commit: -1,
                });
                match channel.append_entries(request).await {
                    Ok(res) => {
                        log::info!("{:?}", res)
                    }
                    Err(err) => {
                        log::error!("send request error {}", err)
                    }
                }
                let now = time::Instant::now();
                clone_tx.send(now - pre).expect("send error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
    rx
}
