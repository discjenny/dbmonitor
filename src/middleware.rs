use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use std::time::Instant;

pub async fn logger(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();

    let response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status();

    println!(
        "{} {} {} - {:?}",
        method,
        uri,
        status,
        duration
    );

    response
} 