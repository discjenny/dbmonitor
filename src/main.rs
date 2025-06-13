use axum::{
    middleware as axum_mw,
    routing::{get, post},
    Router,
};

mod routes;
mod middleware;
mod database;
mod token;
mod websocket;
use middleware as mw;

#[tokio::main]
async fn main() {
    let db_pool = database::init_db().await.expect("database connection failed");
    database::run_migrations(&db_pool).await.expect("database migrations failed");
    
    // protected log routes with auth middleware
    let log_routes = Router::new()
        .route("/api/logs", get(routes::api::get_logs))
        .route("/api/logs", post(routes::api::add_log))
        .layer(axum_mw::from_fn(mw::device_auth));

    let app = Router::new()
        .route("/", get(routes::pages::home))
        .route("/dashboard", get(routes::pages::dashboard))
        .route("/api/db-status", get(routes::api::db_status))
        .route("/api/auth", get(routes::api::auth))
        // Removed /api/chart-data - chart data now sent via WebSocket OOB
        // Removed /fragments/current-reading - current reading sent via WebSocket OOB
        .route("/fragments/active-devices", get(routes::api::active_devices_fragment))
        .route("/ws", get(websocket::websocket_handler))
        .merge(log_routes)
        .fallback(routes::pages::not_found)
        .layer(axum_mw::from_fn(mw::logger))
        .with_state(db_pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3010").await.unwrap();
    println!("server running on http://127.0.0.1:3010");
    axum::serve(listener, app).await.unwrap();
}