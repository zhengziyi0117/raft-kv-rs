use raft_kv_rs::raft_proto::{
    raft_service_client::RaftServiceClient, RequestVoteArgs, RequestVoteReply,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = RaftServiceClient::connect("http://127.0.0.1:8080")
        .await
        .unwrap();
    let request = tonic::Request::new(RequestVoteArgs {
        term: 0,
        candidate_id: 0,
        last_log_index: 0,
        last_log_term: 0,
    });
    let response = client.request_vote(request).await.unwrap();
    let reply = response.get_ref();
    println!("RESPONSE={:?}", reply);

    Ok(())
}
