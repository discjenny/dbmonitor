use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::fs;

pub async fn home() -> Html<String> {
    let html_content = fs::read_to_string("templates/home.html")
        .unwrap_or_else(|_| "<h1>Error loading home page</h1>".to_string());
    Html(html_content)
}

pub async fn dashboard() -> Html<String> {
    let html_content = fs::read_to_string("templates/dashboard.html")
        .unwrap_or_else(|_| "<h1>Error loading dashboard</h1>".to_string());
    Html(html_content)
}

pub async fn not_found() -> Response {
    let html_content = fs::read_to_string("templates/error.html")
        .unwrap_or_else(|_| "<h1>404 - Page Not Found</h1>".to_string());
    
    (StatusCode::NOT_FOUND, Html(html_content)).into_response()
} 