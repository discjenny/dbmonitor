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
mod cache;
use middleware as mw;

#[tokio::main]
async fn main() {
    let db_pool = database::init_db().await.expect("database connection failed");
    database::run_migrations(&db_pool).await.expect("database migrations failed");
    
    cache::init_batch_processor(db_pool.clone()).await;
    
    // cache cleanup task
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            cache::cleanup_old_entries().await;
        }
    });
    
    // jwt required routes
    let log_routes = Router::new()
        .route("/api/logs", get(routes::api::get_logs))
        .route("/api/logs", post(routes::api::add_log))
        .layer(axum_mw::from_fn(mw::device_auth));

    let app = Router::new()
        .route("/", get(routes::pages::dashboard))
        .route("/api/db-status", get(routes::api::db_status))
        .route("/api/cache-status", get(routes::api::cache_status))
        .route("/api/auth", get(routes::api::auth)) // need to add password or something to this route
        .route("/fragments/active-devices", get(routes::api::active_devices_fragment))
        .route("/static/computed.css", get(routes::pages::serve_css))
        .route("/ws", get(websocket::websocket_handler))
        .merge(log_routes)
        .fallback(routes::pages::not_found)
        .layer(axum_mw::from_fn(mw::logger))
        .with_state(db_pool);

    let listener = tokio::net::TcpListener::bind("192.168.1.134:3010").await.unwrap();
    println!("server running on http://192.168.1.134:3010");
    axum::serve(listener, app).await.unwrap();
}