use axum::{
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    // initialize tracing

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/test", get(test));

    // run our app with hyper
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn test() -> Result<Json<(i32, bool)>, StatusCode> {
    let (tx, rx) = oneshot::channel();
    tx.send(1).map_err(|e| StatusCode::ACCEPTED)?;
    // let term = rx.await.map_err(|e| StatusCode::ALREADY_REPORTED)?;
    Ok(Json((2, true)))
}
