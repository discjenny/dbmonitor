use axum::{
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use std::fs;

pub async fn dashboard() -> Html<String> {
    let html_content = fs::read_to_string("static/dashboard.html")
        .unwrap_or_else(|_| "<h1>Error loading dashboard</h1>".to_string());
    Html(html_content)
}

pub async fn serve_css() -> Response {
    let css_content = fs::read_to_string("static/computed.css")
        .unwrap_or_else(|_| "/* Error loading CSS */".to_string());
    
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css")],
        css_content,
    ).into_response()
}

pub async fn not_found() -> Response {
    let html_content = fs::read_to_string("static/error.html")
        .unwrap_or_else(|_| "<h1>404 - Page Not Found</h1>".to_string());
    
    (StatusCode::NOT_FOUND, Html(html_content)).into_response()
} 