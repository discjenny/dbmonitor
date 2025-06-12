use axum::{
    extract::{State, Json, Extension},
    http::{StatusCode, header::{HeaderMap, HeaderName, HeaderValue}},
    response::Json as JsonResponse,
    response::IntoResponse,
};
use crate::database::DbPool;
use serde_json::json;
use serde::Deserialize;
use crate::token;

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
        Ok(rows_affected) => Ok(JsonResponse(json!({
            "status": "success",
            "message": "Decibel log added",
            "rows_affected": rows_affected
        }))),
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