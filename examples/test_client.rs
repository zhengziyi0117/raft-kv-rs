use std::time::{self, Duration};

use raft_kv_rs::raft_proto::{raft_service_client::RaftServiceClient, AppendEntriesArgs};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tonic::transport::{Channel, Uri};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let uri = Uri::builder()
        .scheme("http")
        .authority("127.0.0.1:8000")
        .path_and_query("")
        .build()
        .unwrap();
    let client = RaftServiceClient::connect(uri).await.unwrap();
    request_batch(client).await;
    Ok(())
}

async fn request_batch(client: RaftServiceClient<Channel>) {
    let mut handles = vec![];
    for _ in 0..10 {
        let mut channel = client.clone();
        let handle = tokio::spawn(async move {
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
                        let now = time::Instant::now();
                        log::info!("time:{:?} [{:?}]", now - pre, res)
                    }
                    Err(err) => {
                        let now = time::Instant::now();
                        log::error!("time:{:?} [{}]", now - pre, err)
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await.unwrap();
    }
}
