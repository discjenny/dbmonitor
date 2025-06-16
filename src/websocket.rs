use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::broadcast;
use crate::database::DbPool;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::LazyLock;
use chrono::{DateTime, Utc};

static BROADCAST: LazyLock<broadcast::Sender<String>> = 
    LazyLock::new(|| {
        let (tx, _) = broadcast::channel(1000);
        
        start_throttling_processor(tx.clone());
        
        tx
    });

#[derive(Clone, Debug)]
struct ThrottledReading {
    device_id: i32,
    decibels: f64,
    timestamp: DateTime<Utc>,
}

static PENDING_READINGS: LazyLock<RwLock<HashMap<i32, ThrottledReading>>> = 
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn start_throttling_processor(sender: broadcast::Sender<String>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        
        loop {
            interval.tick().await;
            
            let readings = {
                let mut pending = PENDING_READINGS.write().await;
                let current_readings: Vec<ThrottledReading> = pending.values().cloned().collect();
                pending.clear();
                current_readings
            };
            
            // send updates for all devices that had readings in this window
            for reading in readings {
                let timestamp = reading.timestamp;
                
                // current reading oob swap, updates display
                let current_reading_fragment = format!(r#"
                    <div id="current-decibels" class="text-6xl font-bold text-primary mb-2" hx-swap-oob="true" data-decibels="{:.1}" data-timestamp="{}" data-device-id="{}">{:.1}</div>"#, 
                    reading.decibels, timestamp.to_rfc3339(), reading.device_id, reading.decibels);
                
                // chart data oob fragment, hidden element with data for chart updates
                let chart_data_fragment = format!(r#"
                    <div id="chart-update" hx-swap-oob="true" 
                        data-decibels="{:.1}" 
                        data-timestamp="{}" 
                        data-device-id="{}" 
                        style="display:none">
                    </div>"#, 
                    reading.decibels, timestamp.to_rfc3339(), reading.device_id);
                
                // combine fragments
                let combined_message = format!("{}\n{}", current_reading_fragment, chart_data_fragment);
                
                let _ = sender.send(combined_message);
            }
        }
    });
}

pub async fn broadcast_reading_update(decibels: f64, device_id: i32) {
    let timestamp = chrono::Utc::now();
    
    // store the latest reading for this device (overwrites previous if exists in this window)
    let reading = ThrottledReading {
        device_id,
        decibels,
        timestamp,
    };
    
    let mut pending = PENDING_READINGS.write().await;
    pending.insert(device_id, reading);
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_pool): State<DbPool>,
) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    
    let mut rx = BROADCAST.subscribe();
    
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });
    
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(_)) => {

                }
                Ok(Message::Close(_)) => break,
                _ => {}
            }
        }
    });
    
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        },
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }
} 