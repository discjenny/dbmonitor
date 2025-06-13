use axum::{
    extract::{State, Json, Extension},
    http::{StatusCode, header::{HeaderMap, HeaderName, HeaderValue}},
    response::Json as JsonResponse,
    response::{IntoResponse, Html},
};
use crate::database::DbPool;
use crate::websocket;
use crate::cache;
use serde_json::json;
use serde::Deserialize;
use crate::token;
use chrono::Utc;

pub async fn db_status(State(pool): State<DbPool>) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let client = match pool.get().await {
        Ok(conn) => conn,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    
    match client.query("SELECT NOW() as current_time, version() as db_version", &[]).await {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                let current_time: chrono::DateTime<chrono::Utc> = row.get("current_time");
                let db_version: &str = row.get("db_version");
                
                Ok(JsonResponse(json!({
                    "status": "connected",
                    "current_time": current_time.to_rfc3339(),
                    "database_version": db_version.split_whitespace().take(2).collect::<Vec<_>>().join(" "),
                    "database_name": "dbmonitor"
                })))
            } else {
                Ok(JsonResponse(json!({
                    "status": "error",
                    "message": "No data returned from database"
                })))
            }
        }
        Err(e) => {
            eprintln!("database query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct NewDecibelLog {
    pub decibels: f64,
}

pub async fn add_log(
    Extension(device_id): Extension<i32>,
    State(_pool): State<DbPool>,
    Json(payload): Json<NewDecibelLog>,
) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let timestamp = chrono::Utc::now();
    
    cache::update_device_reading(device_id, payload.decibels, timestamp).await;
    
    websocket::broadcast_reading_update(payload.decibels, device_id).await;
    
    cache::queue_insert(device_id, payload.decibels, timestamp).await;
    
    Ok(JsonResponse(json!({
        "status": "success",
        "message": "Decibel log queued",
        "cached": true
    })))
}

pub async fn get_logs(State(pool): State<DbPool>) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let client = match pool.get().await {
        Ok(conn) => conn,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    match client
        .query(
            "SELECT id, decibels, created_at, fk_device_id FROM decibel_logs ORDER BY created_at DESC LIMIT 10",
            &[],
        )
        .await
    {
        Ok(rows) => {
            let logs: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|row| {
                    json!({
                        "id": row.get::<_, i32>("id"),
                        "created_at": row.get::<_, chrono::DateTime<chrono::Utc>>("created_at").to_rfc3339(),
                        "decibels": row.get::<_, f64>("decibels"),
                        "device_id": row.get::<_, i32>("fk_device_id"),
                    })
                })
                .collect();

            Ok(JsonResponse(json!({
                "status": "success",
                "logs": logs,
                "count": logs.len()
            })))
        }
        Err(e) => {
            eprintln!("database query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn auth(State(pool): State<DbPool>) -> Result<impl IntoResponse, StatusCode> {
    let client = match pool.get().await {
        Ok(conn) => conn,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let row = match client
        .query_one("INSERT INTO devices DEFAULT VALUES RETURNING id", &[])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("database insert error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let device_id: i32 = row.get("id");

    let token_str = match token::generate_token(device_id) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("token generation error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-device-token"),
        HeaderValue::from_str(&token_str).unwrap(),
    );

    Ok((headers, JsonResponse(json!({ "token": token_str, "device_id": device_id }))))
}

pub async fn active_devices_fragment(State(_pool): State<DbPool>) -> Result<Html<String>, StatusCode> {
    let active_devices = cache::get_active_devices().await;
    
    if active_devices.is_empty() {
        let html = r#"
            <div class="text-center py-8 text-muted-foreground">
                <div class="text-5xl mb-4 opacity-50">📱</div>
                <div>No active devices (no readings in last minute)</div>
            </div>
        "#;
        return Ok(Html(html.to_string()));
    }

    let mut html = String::new();
    let now = Utc::now();

    let mut sorted_devices = active_devices;
    sorted_devices.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    for reading in sorted_devices {
        let seconds_ago = (now - reading.timestamp).num_seconds();
        let time_text = match seconds_ago {
            0 => "just now".to_string(),
            1 => "1 second ago".to_string(),
            n => format!("{} seconds ago", n),
        };

        html.push_str(&format!(r#"
            <div class="flex justify-between items-center p-4 border-b border-border hover:bg-card transition-all">
                <div class="flex flex-col gap-1">
                    <div class="font-bold text-card-foreground text-lg">Device {}</div>
                    <div class="font-bold text-primary text-xl">{:.1} dB</div>
                </div>
                <div class="flex flex-col items-end gap-1">
                    <div class="w-2 h-2 rounded-full bg-accent"></div>
                    <div class="text-xs text-muted-foreground">{}</div>
                </div>
            </div>
        "#, reading.device_id, reading.decibels, time_text));
    }

    Ok(Html(html))
}

pub async fn cache_status(State(_pool): State<DbPool>) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let active_devices = cache::get_active_devices().await;
    let cache_size = cache::cache_size().await;
    let (_, queue_active) = cache::is_queue_active().await;
    
    Ok(JsonResponse(json!({
        "cache_size": cache_size,
        "active_devices": active_devices.len(),
        "batch_processor": {
            "active": queue_active
        },
        "devices": active_devices.iter().map(|d| json!({
            "device_id": d.device_id,
            "decibels": d.decibels,
            "timestamp": d.timestamp.to_rfc3339(),
            "seconds_ago": (chrono::Utc::now() - d.timestamp).num_seconds()
        })).collect::<Vec<_>>()
    })))
} 