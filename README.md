# DBMonitor

IoT server for real-time decibel monitoring. Handles 25,000-75,000 requests per second on a single CPU.

## Features

- High-throughput request handling with intelligent batching
- Real-time WebSocket updates
- JWT device authentication
- HTMX-powered interface
- Optimized for high-frequency sensor data

## Architecture

- Axum web server with PostgreSQL backend
- DashMap for lock-free concurrent caching
- Asynchronous batch processing (up to 200 inserts per batch)
- WebSocket streaming with HTMX frontend

## Setup

Prerequisites: Rust 1.70+, PostgreSQL 12+, Tailwind v4 CLI

```bash
git clone <repository-url>
cd dbmonitor
cargo build --release

# Create database
createdb dbmonitor

# Compute css (if needed)
tailwindcss -i ./input.css -o ./static/computed.css

# Run server
cargo run --release
```

Server runs on `http://127.0.0.1:3010`

## Mock Device

Test with the included sensor simulator:

```bash
cd mock_device
cargo run
```

Generates realistic decibel patterns and sends data every 1ms for stress testing.

## API

```http
POST /api/logs          # Submit readings (requires auth)
GET /api/logs           # Get historical data (requires auth)
GET /api/auth           # Get device token
GET /api/db-status      # Database status
GET /fragments/active-devices  # HTMX fragment
```

WebSocket: `ws://127.0.0.1:3010/ws`

## Configuration

```bash
DB_HOST=localhost
DB_USER=postgres  
DB_PASSWORD=postgres
DB_NAME=dbmonitor
DEVICE_TOKEN_SECRET=69420
``` 