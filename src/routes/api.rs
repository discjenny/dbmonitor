use axum::{
    extract::{State, Json, Extension},
    http::{StatusCode, header::{HeaderMap, HeaderName, HeaderValue}},
    response::Json as JsonResponse,
    response::{IntoResponse, Html},
};
use crate::database::DbPool;
use crate::websocket;
use serde_json::json;
use serde::Deserialize;
use crate::token;
use chrono::{DateTime, Utc};

pub async fn db_status(State(pool): State<DbPool>) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;
    
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
    State(pool): State<DbPool>,
    Json(payload): Json<NewDecibelLog>,
) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;

    match client
        .execute(
            "INSERT INTO decibel_logs (decibels, fk_device_id) VALUES ($1, $2)",
            &[&payload.decibels, &device_id],
        )
        .await
    {
        Ok(rows_affected) => {
            // Broadcast HTML updates to all WebSocket clients
            broadcast_html_updates(payload.decibels, device_id);
            
            Ok(JsonResponse(json!({
                "status": "success",
                "message": "Decibel log added",
                "rows_affected": rows_affected
            })))
        },
        Err(e) => {
            eprintln!("database insert error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_logs(State(pool): State<DbPool>) -> Result<JsonResponse<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;

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
    let client = pool.get_client().await;

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

pub async fn active_devices_fragment(State(pool): State<DbPool>) -> Result<Html<String>, StatusCode> {
    let client = pool.get_client().await;

    // Get devices that have sent readings in the last 60 seconds
    match client
        .query(
            "SELECT DISTINCT fk_device_id as device_id, 
                    FIRST_VALUE(decibels) OVER (PARTITION BY fk_device_id ORDER BY created_at DESC) as latest_decibels,
                    FIRST_VALUE(created_at) OVER (PARTITION BY fk_device_id ORDER BY created_at DESC) as latest_time
             FROM decibel_logs 
             WHERE created_at > NOW() - INTERVAL '60 seconds'
             ORDER BY latest_time DESC",
            &[],
        )
        .await
    {
        Ok(rows) => {
            if rows.is_empty() {
                let html = r#"
                    <div class="empty-state">
                        <div class="icon">📱</div>
                        <div>No active devices (no readings in last minute)</div>
                    </div>
                "#;
                return Ok(Html(html.to_string()));
            }

            let mut html = String::new();
            let now = Utc::now();
            
            for row in rows {
                let device_id: i32 = row.get("device_id");
                let decibels: f64 = row.get("latest_decibels");
                let timestamp: DateTime<Utc> = row.get("latest_time");
                
                let seconds_ago = (now - timestamp).num_seconds();
                let time_text = match seconds_ago {
                    0 => "just now".to_string(),
                    1 => "1 second ago".to_string(),
                    n => format!("{} seconds ago", n),
                };

                html.push_str(&format!(r#"
                    <div class="device-item">
                        <div class="device-info">
                            <div class="device-id">Device {}</div>
                            <div class="device-reading">{:.1} dB</div>
                        </div>
                        <div class="device-status">
                            <div class="activity-indicator"></div>
                            <div class="last-seen">{}</div>
                        </div>
                    </div>
                "#, device_id, decibels, time_text));
            }

            Ok(Html(html))
        }
        Err(e) => {
            eprintln!("database query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// WebSocket message handler that sends HTML fragments
pub fn broadcast_html_updates(decibels: f64, device_id: i32) {
    let timestamp = chrono::Utc::now();
    
    // 1. Current reading OOB swap - updates the display
    let current_reading_fragment = format!(r#"
<div id="current-decibels" class="decibel-value" hx-swap-oob="true" data-decibels="{:.1}" data-timestamp="{}" data-device-id="{}">{:.1}</div>"#, 
        decibels, timestamp.to_rfc3339(), device_id, decibels);
    
    // 2. Chart data OOB fragment - hidden element with data for chart updates
    // This follows the recommended pattern: OOB data fragment + JS event
    let chart_data_fragment = format!(r#"
<div id="chart-update" hx-swap-oob="true" 
     data-decibels="{:.1}" 
     data-timestamp="{}" 
     data-device-id="{}" 
     style="display:none"></div>"#, 
        decibels, timestamp.to_rfc3339(), device_id);
    
    // Combine both fragments
    let combined_message = format!("{}\n{}", current_reading_fragment, chart_data_fragment);
    
    websocket::broadcast_message(combined_message);
} 