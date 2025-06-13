-- Optimize for high-throughput inserts

-- Add composite index for efficient queries on device + time
CREATE INDEX IF NOT EXISTS idx_decibel_logs_device_time 
ON decibel_logs(fk_device_id, created_at DESC);

-- Optimize table for append-heavy workloads
ALTER TABLE decibel_logs SET (fillfactor = 100);

-- Enable parallel writes (PostgreSQL 13+)
ALTER TABLE decibel_logs SET (parallel_workers = 4); 