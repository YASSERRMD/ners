# NERS - High-Performance Web Server Kernel

A lock-free, single-threaded web server kernel written in Rust using a 6-stage pipeline architecture.

## Architecture

```
Network → [NetIn] → [Parse] → [Route] → [App] → [Encode] → [NetOut] → Network

         Shared Memory
      +----------------+
      | Slab<ConnState>|  (all connection buffers & metadata)
      | MetricsSnapshot|  (per-stage metrics)
      +----------------+
```

## Features

- **Lock-free**: No mutexes in the critical path
- **Zero-copy**: Slab allocator for connection state
- **Observable**: Built-in per-stage metrics
- **Fast**: Target >5k req/sec on single thread

## Project Structure

```
ners/
├── ners-core/           # Core kernel and stages
│   ├── src/
│   │   ├── conn.rs      # Connection slab
│   │   ├── queue.rs     # Lock-free ring queue
│   │   ├── net.rs       # TCP I/O
│   │   ├── stage.rs     # 6-stage pipeline
│   │   ├── handlers.rs  # Route handlers
│   │   └── main.rs      # Event loop
│   ├── tests/
│   └── benches/
├── ners-proto-http/     # HTTP/1.1 parser
└── ners-metrics/        # Metrics collection
```

## Quick Start

### Build

```bash
cargo build --release
```

### Run Server

```bash
cargo run --release
# Listening on 0.0.0.0:8080
```

### Test Endpoints

```bash
curl http://localhost:8080/
# Hello, World!

curl http://localhost:8080/api/test
# {"status": "ok", "message": "NERS Phase 1"}

curl http://localhost:8080/nonexistent
# 404 Not Found
```

### Run Tests

```bash
cargo test
```

### Run Benchmarks

```bash
# Start server in one terminal, then:
cargo bench --bench phase1
```

## Phase 1 Goals

- [x] Single-threaded event loop
- [x] 6-stage pipeline (NetIn, Parse, Route, App, Encode, NetOut)
- [x] Lock-free inter-stage queues
- [x] Non-blocking TCP I/O
- [x] HTTP/1.1 parser
- [x] Per-stage metrics collection

## License

MIT License - see LICENSE file
