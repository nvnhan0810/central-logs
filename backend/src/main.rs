use central_logs_backend::interfaces::http::{AppState, router};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::var("CENTRAL_LOGS_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("central-logs backend listening on http://{addr}");
    axum::serve(listener, router(AppState::in_memory())).await
}
