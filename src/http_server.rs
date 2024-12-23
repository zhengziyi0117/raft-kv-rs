use std::{net::SocketAddr, sync::Arc};

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;
use tokio::sync::{broadcast::Receiver, mpsc::UnboundedSender, oneshot};

use crate::core::RaftMessage;

#[derive(Serialize, Debug)]
pub struct RaftNodeStatusResponse {
    pub current_term: i32,
    pub is_leader: bool,
}

pub trait RaftHttpApi {
    fn get_status(&self) -> impl std::future::Future<Output = Result<Json<RaftNodeStatusResponse>, StatusCode>> + Send;
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
        let listener = tokio::net::TcpListener::bind(self.bind_addr).await.unwrap();
        let mut shutdown = self.shutdown.resubscribe();
        
        let app = Router::new()
            .route("/get_status", get(RaftHttpServer::handle_get_status))
            .with_state(Arc::new(self));
        
        axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        }).await.unwrap();
    }

    // 这个方法作为实际的 handler
    async fn handle_get_status(
        State(state): State<Arc<Self>>,
    ) -> Result<Json<RaftNodeStatusResponse>, StatusCode> {
        state.get_status().await
    }
}

impl RaftHttpApi for RaftHttpServer {
    async fn get_status(&self) -> Result<Json<RaftNodeStatusResponse>, StatusCode> {
        let (tx, rx) = oneshot::channel();
        self.tx_api
            .send(RaftMessage::GetStatusRequest(tx))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let resp = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(resp))
    }
}
