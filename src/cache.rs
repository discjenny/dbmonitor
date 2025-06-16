use chrono::{DateTime, Utc};
use std::sync::LazyLock;
use tokio::sync::mpsc;
use crate::database::DbPool;
use dashmap::DashMap;

#[derive(Clone, Debug)]
pub struct DeviceReading {
    pub device_id: i32,
    pub decibels: f64,
    pub timestamp: DateTime<Utc>,
}

static ACTIVE_DEVICES: LazyLock<DashMap<i32, DeviceReading>> = LazyLock::new(|| {
    DashMap::new()
});

#[derive(Clone, Debug)]
pub struct PendingInsert {
    pub device_id: i32,
    pub decibels: f64,
    pub timestamp: DateTime<Utc>,
}

static INSERT_QUEUE: LazyLock<tokio::sync::Mutex<Option<mpsc::UnboundedSender<PendingInsert>>>> = 
    LazyLock::new(|| tokio::sync::Mutex::new(None));

pub async fn init_batch_processor(pool: DbPool) {
    let (tx, rx) = mpsc::unbounded_channel::<PendingInsert>();
    
    {
        let mut queue = INSERT_QUEUE.lock().await;
        *queue = Some(tx);
    }
    
    tokio::spawn(batch_insert_processor(rx, pool));
    
    // println!("batch insert processor initializd for high-throughput operations");
}

async fn batch_insert_processor(mut rx: mpsc::UnboundedReceiver<PendingInsert>, pool: DbPool) {
    let mut batch = Vec::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(50)); // process every 50ms for high throughput
    let mut max_wait_timer = tokio::time::interval(std::time::Duration::from_millis(200)); // max 200ms wait for small batches
    // let mut stats_interval = tokio::time::interval(std::time::Duration::from_secs(10)); // stats every 10s
    
    // let mut total_processed = 0u64;
    // let mut total_batches = 0u64;
    
    loop {
        tokio::select! {
            maybe_insert = rx.recv() => {
                if let Some(insert) = maybe_insert {
                    batch.push(insert);
                    
                    // for high throughput, process larger batches
                    if batch.len() >= 200 {
                        let _processed = process_batch(&mut batch, &pool).await;
                        // total_processed += processed;
                        // if processed > 0 {
                        //     total_batches += 1;
                        // }
                    }
                } else {
                    // channel closed, process remaining batch
                    if !batch.is_empty() {
                        let _processed = process_batch(&mut batch, &pool).await;
                        // total_processed += processed;
                        // if processed > 0 {
                        //     total_batches += 1;
                        // }
                    }
                    break;
                }
            }
            
            // for low throughput, process batch on timer 
            _ = interval.tick() => {
                if batch.len() >= 10 {  // minimum batch size for timer processing
                    let _processed = process_batch(&mut batch, &pool).await;
                    // total_processed += processed;
                    // if processed > 0 {
                    //     total_batches += 1;
                    // }
                }
            }
            
            // force process any pending items after max wait time 
            _ = max_wait_timer.tick() => {
                if !batch.is_empty() {
                    let _processed = process_batch(&mut batch, &pool).await;
                    // total_processed += processed;
                    // if processed > 0 {
                    //     total_batches += 1;
                    // }
                }
            }
            
            // print stats periodically
            // _ = stats_interval.tick() => {
            //     if total_batches > 0 {
            //         println!("batch processor stats: {} inserts in {} batches (avg {:.1} per batch)", 
            //             total_processed, total_batches, 
            //             total_processed as f64 / total_batches as f64);
            //     }
            // }
        }
    }
    
    // println!("batch processor shutdown - processed {} total inserts in {} batches", total_processed, total_batches);
}

async fn process_batch(batch: &mut Vec<PendingInsert>, pool: &DbPool) -> u64 {
    if batch.is_empty() {
        return 0;
    }
    
    let batch_size = batch.len();
    
    match pool.get().await {
        Ok(client) => {
            if batch_size == 1 {
                // single insert for small batches
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
                // bulk insert for larger batches
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
            // dont clear batch on connection errors - we'll retry next time
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
    
    // get sender and queue the insert
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

pub async fn is_queue_active() -> (usize, bool) {
    let queue = INSERT_QUEUE.lock().await;
    match queue.as_ref() {
        Some(_sender) => {
            (0, true) // 
        }
        None => (0, false)
    }
} 