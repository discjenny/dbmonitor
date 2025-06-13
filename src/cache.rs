use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use crate::database::DbPool;
use dashmap::DashMap;

#[derive(Clone, Debug)]
pub struct DeviceReading {
    pub device_id: i32,
    pub decibels: f64,
    pub timestamp: DateTime<Utc>,
}

// High-performance concurrent cache using DashMap (internally sharded)
static ACTIVE_DEVICES: Lazy<DashMap<i32, DeviceReading>> = Lazy::new(|| {
    DashMap::new()
});

// Batch insert system
#[derive(Clone, Debug)]
pub struct PendingInsert {
    pub device_id: i32,
    pub decibels: f64,
    pub timestamp: DateTime<Utc>,
}

static INSERT_QUEUE: Lazy<tokio::sync::Mutex<Option<mpsc::UnboundedSender<PendingInsert>>>> = 
    Lazy::new(|| tokio::sync::Mutex::new(None));

pub async fn init_batch_processor(pool: DbPool) {
    let (tx, rx) = mpsc::unbounded_channel::<PendingInsert>();
    
    // Store the sender globally
    {
        let mut queue = INSERT_QUEUE.lock().await;
        *queue = Some(tx);
    }
    
    // Spawn background batch processor with the pool
    tokio::spawn(batch_insert_processor(rx, pool));
    
    println!("batch insert processor initialized for high-throughput operations");
}

async fn batch_insert_processor(mut rx: mpsc::UnboundedReceiver<PendingInsert>, pool: DbPool) {
    let mut batch = Vec::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(50)); // Process every 50ms for high throughput
    let mut max_wait_timer = tokio::time::interval(std::time::Duration::from_millis(200)); // Max 200ms wait even for small batches
    let mut stats_interval = tokio::time::interval(std::time::Duration::from_secs(10)); // Stats every 10s
    
    let mut total_processed = 0u64;
    let mut total_batches = 0u64;
    
    loop {
        tokio::select! {
            // Collect pending inserts
            maybe_insert = rx.recv() => {
                if let Some(insert) = maybe_insert {
                    batch.push(insert);
                    
                    // For high throughput, process larger batches
                    if batch.len() >= 200 {
                        let processed = process_batch(&mut batch, &pool).await;
                        total_processed += processed;
                        if processed > 0 {
                            total_batches += 1;
                        }
                    }
                } else {
                    // Channel closed, process remaining batch
                    if !batch.is_empty() {
                        let processed = process_batch(&mut batch, &pool).await;
                        total_processed += processed;
                        if processed > 0 {
                            total_batches += 1;
                        }
                    }
                    break;
                }
            }
            
            // Process batch on timer (only if we have a reasonable batch size)
            _ = interval.tick() => {
                if batch.len() >= 10 {  // Minimum batch size for timer processing
                    let processed = process_batch(&mut batch, &pool).await;
                    total_processed += processed;
                    if processed > 0 {
                        total_batches += 1;
                    }
                }
            }
            
            // Force process any pending items after max wait time (prevents UI lag)
            _ = max_wait_timer.tick() => {
                if !batch.is_empty() {
                    let processed = process_batch(&mut batch, &pool).await;
                    total_processed += processed;
                    if processed > 0 {
                        total_batches += 1;
                    }
                }
            }
            
            // Print stats periodically
            _ = stats_interval.tick() => {
                if total_batches > 0 {
                    println!("batch processor stats: {} inserts in {} batches (avg {:.1} per batch)", 
                        total_processed, total_batches, 
                        total_processed as f64 / total_batches as f64);
                }
            }
        }
    }
    
    println!("batch processor shutdown - processed {} total inserts in {} batches", total_processed, total_batches);
}

async fn process_batch(batch: &mut Vec<PendingInsert>, pool: &DbPool) -> u64 {
    if batch.is_empty() {
        return 0;
    }
    
    let batch_size = batch.len();
    
    match pool.get().await {
        Ok(client) => {
            // Use PostgreSQL's bulk insert with VALUES for maximum performance
            if batch_size == 1 {
                // Single insert for small batches
                let insert = &batch[0];
                match client
                    .execute(
                        "INSERT INTO decibel_logs (decibels, fk_device_id, created_at) VALUES ($1, $2, $3)",
                        &[&insert.decibels, &insert.device_id, &insert.timestamp],
                    )
                    .await
                {
                    Ok(_) => {
                        batch.clear();
                        return 1;
                    }
                    Err(e) => {
                        eprintln!("single insert error: {}", e);
                        batch.clear();
                        return 0;
                    }
                }
            } else {
                // Bulk insert for larger batches
                let mut query = String::from("INSERT INTO decibel_logs (decibels, fk_device_id, created_at) VALUES ");
                let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
                
                for (i, insert) in batch.iter().enumerate() {
                    if i > 0 {
                        query.push_str(", ");
                    }
                    let base = i * 3;
                    query.push_str(&format!("(${}, ${}, ${})", base + 1, base + 2, base + 3));
                    
                    params.push(&insert.decibels);
                    params.push(&insert.device_id);
                    params.push(&insert.timestamp);
                }
                
                match client.execute(&query, &params).await {
                    Ok(rows_inserted) => {
                        let processed = rows_inserted as u64;
                        batch.clear();
                        return processed;
                    }
                    Err(e) => {
                        eprintln!("batch insert error (size {}): {}", batch_size, e);
                        batch.clear();
                        return 0;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("failed to get database connection for batch insert: {}", e);
            // Don't clear batch on connection errors - we'll retry next time
            return 0;
        }
    }
}

pub async fn update_device_reading(device_id: i32, decibels: f64, timestamp: DateTime<Utc>) {
    let reading = DeviceReading {
        device_id,
        decibels,
        timestamp,
    };
    
    ACTIVE_DEVICES.insert(device_id, reading);
}

pub async fn queue_insert(device_id: i32, decibels: f64, timestamp: DateTime<Utc>) {
    let insert = PendingInsert {
        device_id,
        decibels,
        timestamp,
    };
    
    // Get the sender and queue the insert
    let queue = INSERT_QUEUE.lock().await;
    if let Some(sender) = queue.as_ref() {
        if let Err(_) = sender.send(insert) {
            eprintln!("warning: insert queue channel closed");
        }
    } else {
        eprintln!("warning: batch processor not initialized");
    }
}

pub async fn get_active_devices() -> Vec<DeviceReading> {
    let now = Utc::now();
    let cutoff = now - chrono::Duration::seconds(60);
    
    ACTIVE_DEVICES
        .iter()
        .filter(|entry| entry.value().timestamp > cutoff)
        .map(|entry| entry.value().clone())
        .collect()
}

pub async fn cleanup_old_entries() {
    let now = Utc::now();
    let cutoff = now - chrono::Duration::minutes(5);
    
    ACTIVE_DEVICES.retain(|_, reading| reading.timestamp > cutoff);
}

pub async fn cache_size() -> usize {
    ACTIVE_DEVICES.len()
}

// Get queue status for monitoring
pub async fn get_queue_stats() -> (usize, bool) {
    let queue = INSERT_QUEUE.lock().await;
    match queue.as_ref() {
        Some(_sender) => {
            // Approximate queue size (not exact but good enough for monitoring)
            (0, true) // Channel doesn't expose len(), but we know it's active
        }
        None => (0, false)
    }
} 