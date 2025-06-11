use axum::{
    middleware as axum_mw,
    routing::get,
    Router,
};

mod routes;
mod middleware;
use middleware as mw;

#[tokio::main]
async fn main() {
    // build our application with routes
    let app = Router::new()
        .route("/", get(routes::pages::home))
        .route("/hello", get(routes::pages::hello))
        .fallback(routes::pages::not_found)
        .layer(axum_mw::from_fn(mw::logger));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("🚀 Server running on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}