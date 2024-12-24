use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast::Receiver, mpsc::UnboundedSender, oneshot};

use crate::{
    core::RaftMessage,
    raft_proto::{CommandType, Entry},
};

#[derive(Serialize, Debug)]
pub struct RaftNodeStatusResponse {
    pub current_term: u32,
    pub is_leader: bool,
}

#[derive(Serialize, Debug)]
pub struct RaftAddLogResponse {
    pub log_index: u32,
    pub current_term: u32,
    pub is_leader: bool,
}

#[derive(Deserialize, Debug)]
pub struct RaftAddLogRequest {
    pub command_type: String,
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Debug)]
pub struct RaftLogListResponse {
    pub last_applied: u32,
    pub commit_index: u32,
    pub logs: Vec<Entry>,
}

pub trait RaftHttpApi {
    // get
    fn handle_get_status(
        state: State<Arc<RaftHttpServer>>,
    ) -> impl std::future::Future<Output = Result<Json<RaftNodeStatusResponse>, StatusCode>> + Send;

    fn handle_list_logs(
        state: State<Arc<RaftHttpServer>>,
    ) -> impl std::future::Future<Output = Result<Json<RaftLogListResponse>, StatusCode>> + Send;

    // post
    fn handle_add_log(
        state: State<Arc<RaftHttpServer>>,
        request: Json<RaftAddLogRequest>,
    ) -> impl std::future::Future<Output = Result<Json<RaftAddLogResponse>, StatusCode>> + Send;
}

// 用于暴露用户查询或者添加日志等接口
pub struct RaftHttpServer {
    tx_api: UnboundedSender<RaftMessage>,
    bind_addr: SocketAddr,
    shutdown: Receiver<()>,
}

impl RaftHttpServer {
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

    pub async fn start(self) {
        log::info!("raft http server start {:?}", self.bind_addr);
        let listener = tokio::net::TcpListener::bind(self.bind_addr).await.unwrap();
        let mut shutdown = self.shutdown.resubscribe();

        let app = Router::new()
            .route("/get_status", get(RaftHttpServer::handle_get_status))
            .route("/add_log", post(RaftHttpServer::handle_add_log))
            .route("/debug/list_logs", get(RaftHttpServer::handle_list_logs))
            .with_state(Arc::new(self));

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.recv().await;
            })
            .await
            .unwrap();
    }
}

impl RaftHttpApi for RaftHttpServer {
    async fn handle_get_status(
        State(state): State<Arc<RaftHttpServer>>,
    ) -> Result<Json<RaftNodeStatusResponse>, StatusCode> {
        let (tx, rx) = oneshot::channel();
        state
            .tx_api
            .send(RaftMessage::GetStatusRequest(tx))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let resp = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(resp))
    }

    async fn handle_add_log(
        State(state): State<Arc<RaftHttpServer>>,
        Json(entry): Json<RaftAddLogRequest>,
    ) -> Result<Json<RaftAddLogResponse>, StatusCode> {
        let (tx, rx) = oneshot::channel();
        // 懒得新开一个struct用于传输了，这里仅仅用来校验command_type对不对，这里校验之后后面在core里面直接unwrap即可
        let _command_type = CommandType::from_str_name(&entry.command_type)
            .ok_or_else(|| StatusCode::BAD_REQUEST)?;
        state
            .tx_api
            .send(RaftMessage::AddLogRequest(entry, tx))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let resp = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(resp))
    }
    
    async fn handle_list_logs(
        state: State<Arc<RaftHttpServer>>,
    ) -> Result<Json<RaftLogListResponse>, StatusCode> {
        let (tx, rx) = oneshot::channel();
        state
            .tx_api
            .send(RaftMessage::ListLogsRequest(tx))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let resp = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(resp))
    }
}
