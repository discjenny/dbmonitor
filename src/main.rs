use axum::{
    middleware as axum_mw,
    routing::{get, post},
    Router,
};

mod routes;
mod middleware;
mod database;
use middleware as mw;

#[tokio::main]
async fn main() {
    let db_pool = database::init_db().await.expect("database connection failed");
    database::run_migrations(&db_pool).await.expect("database migrations failed");
    
    let app = Router::new()
        .route("/", get(routes::pages::home))
        .route("/hello", get(routes::pages::hello))
        .route("/api/db-status", get(routes::api::db_status))
        .route("/api/logs", get(routes::api::get_logs))
        .route("/api/logs", post(routes::api::add_log))
        .fallback(routes::pages::not_found)
        .layer(axum_mw::from_fn(mw::logger))
        .with_state(db_pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3010").await.unwrap();
    println!("server running on http://127.0.0.1:3010");
    axum::serve(listener, app).await.unwrap();
}