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

// Global broadcast channel for WebSocket messages
static BROADCAST: once_cell::sync::Lazy<broadcast::Sender<String>> = 
    once_cell::sync::Lazy::new(|| {
        let (tx, _) = broadcast::channel(100);
        tx
    });



// Generic message broadcast function
pub fn broadcast_message(message: String) {
    // Ignore errors (no subscribers)
    let _ = BROADCAST.send(message);
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