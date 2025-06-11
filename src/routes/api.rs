use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use crate::database::DbPool;
use serde_json::json;

pub async fn db_status(State(pool): State<DbPool>) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;
    
    match client.query("SELECT NOW() as current_time, version() as db_version", &[]).await {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                let current_time: chrono::DateTime<chrono::Utc> = row.get("current_time");
                let db_version: &str = row.get("db_version");
                
                Ok(Json(json!({
                    "status": "connected",
                    "current_time": current_time.to_rfc3339(),
                    "database_version": db_version.split_whitespace().take(2).collect::<Vec<_>>().join(" "),
                    "database_name": "dbmonitor"
                })))
            } else {
                Ok(Json(json!({
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

pub async fn add_log(State(pool): State<DbPool>) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;
    
    match client.execute(
        "INSERT INTO monitor_logs (level, message, source) VALUES ($1, $2, $3)",
        &[&"INFO", &"Test log entry from API", &"web-api"]
    ).await {
        Ok(rows_affected) => {
            Ok(Json(json!({
                "status": "success",
                "message": "Log entry added",
                "rows_affected": rows_affected
            })))
        }
        Err(e) => {
            eprintln!("database insert error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_logs(State(pool): State<DbPool>) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = pool.get_client().await;
    
    match client.query(
        "SELECT id, timestamp, level, message, source FROM monitor_logs ORDER BY timestamp DESC LIMIT 10", 
        &[]
    ).await {
        Ok(rows) => {
            let logs: Vec<serde_json::Value> = rows.into_iter().map(|row| {
                json!({
                    "id": row.get::<_, i32>("id"),
                    "timestamp": row.get::<_, chrono::DateTime<chrono::Utc>>("timestamp").to_rfc3339(),
                    "level": row.get::<_, &str>("level"),
                    "message": row.get::<_, &str>("message"),
                    "source": row.get::<_, Option<&str>>("source")
                })
            }).collect();
            
            Ok(Json(json!({
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