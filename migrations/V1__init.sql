CREATE TABLE devices (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    deleted_at TIMESTAMPTZ
);

CREATE TABLE decibel_logs (
    id SERIAL PRIMARY KEY,
    decibels DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fk_device_id INTEGER NOT NULL REFERENCES devices(id)
);

CREATE INDEX idx_decibel_logs_fk_device_id ON decibel_logs(fk_device_id);
CREATE INDEX idx_decibel_logs_created_at ON decibel_logs(created_at);
CREATE INDEX idx_decibel_logs_decibels ON decibel_logs(decibels);
