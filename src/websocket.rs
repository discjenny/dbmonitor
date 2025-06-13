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
use once_cell::sync::Lazy;
use chrono::{DateTime, Utc};

// Global broadcast channel for WebSocket messages
static BROADCAST: Lazy<broadcast::Sender<String>> = 
    Lazy::new(|| {
        let (tx, _) = broadcast::channel(1000); // Increased for high throughput
        
        // Start the throttling processor
        start_throttling_processor(tx.clone());
        
        tx
    });

// Throttled readings per device
#[derive(Clone, Debug)]
struct ThrottledReading {
    device_id: i32,
    decibels: f64,
    timestamp: DateTime<Utc>,
}

// Pending readings to be throttled
static PENDING_READINGS: Lazy<RwLock<HashMap<i32, ThrottledReading>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

fn start_throttling_processor(sender: broadcast::Sender<String>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100)); // 100ms throttle window
        
        loop {
            interval.tick().await;
            
            // Get all pending readings and clear the map
            let readings = {
                let mut pending = PENDING_READINGS.write().await;
                let current_readings: Vec<ThrottledReading> = pending.values().cloned().collect();
                pending.clear();
                current_readings
            };
            
            // Send updates for all devices that had readings in this window
            for reading in readings {
                let timestamp = reading.timestamp;
                
                // 1. Current reading OOB swap - updates the display
                let current_reading_fragment = format!(r#"
<div id="current-decibels" class="text-6xl font-bold text-primary mb-2" hx-swap-oob="true" data-decibels="{:.1}" data-timestamp="{}" data-device-id="{}">{:.1}</div>"#, 
                    reading.decibels, timestamp.to_rfc3339(), reading.device_id, reading.decibels);
                
                // 2. Chart data OOB fragment - hidden element with data for chart updates
                let chart_data_fragment = format!(r#"
<div id="chart-update" hx-swap-oob="true" 
     data-decibels="{:.1}" 
     data-timestamp="{}" 
     data-device-id="{}" 
     style="display:none"></div>"#, 
                    reading.decibels, timestamp.to_rfc3339(), reading.device_id);
                
                // Combine both fragments
                let combined_message = format!("{}\n{}", current_reading_fragment, chart_data_fragment);
                
                // Send via broadcast (ignore errors if no subscribers)
                let _ = sender.send(combined_message);
            }
        }
    });
}

// Updated broadcast function that throttles updates
pub async fn broadcast_reading_update(decibels: f64, device_id: i32) {
    let timestamp = chrono::Utc::now();
    
    // Store the latest reading for this device (overwrites previous if exists in this window)
    let reading = ThrottledReading {
        device_id,
        decibels,
        timestamp,
    };
    
    let mut pending = PENDING_READINGS.write().await;
    pending.insert(device_id, reading);
}

// WebSocket upgrade handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_pool): State<DbPool>,
) -> Response {
    ws.on_upgrade(handle_socket)
}

// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    
    // Subscribe to broadcast channel
    let mut rx = BROADCAST.subscribe();
    
    // Spawn task to handle outgoing messages
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });
    
    // Handle incoming messages (mostly just pings/pongs)
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(_)) => {
                    // Echo back or handle commands if needed
                }
                Ok(Message::Close(_)) => break,
                _ => {}
            }
        }
    });
    
    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        },
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }
} 