use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
    http::StatusCode,
    response::IntoResponse,
};
use std::time::Instant;
use crate::token;

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

// verifies `Authorization: Bearer <token>` header and injects `device_id: i32` into request extensions
pub async fn device_auth(mut req: Request, next: Next) -> Response {
    let Some(auth_header) = req.headers().get("Authorization") else {
        return (StatusCode::UNAUTHORIZED, "missing authorization header").into_response();
    };

    let Ok(auth_str) = auth_header.to_str() else {
        return (StatusCode::UNAUTHORIZED, "invalid authorization header").into_response();
    };

    let token = auth_str.strip_prefix("Bearer ").unwrap_or("");
    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, "invalid bearer token").into_response();
    }

    match token::verify_token(token) {
        Ok(claims) => {
            req.extensions_mut().insert(claims.device_id);
            next.run(req).await
        }
        Err(_) => (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response(),
    }
} 