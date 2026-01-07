# NERS - High-Performance Web Server Kernel

A lock-free, multi-core web server kernel written in Rust with a 6-stage pipeline architecture.

## Architecture

```
Network → [NetIn] → [Parse] → [Route] → [App] → [Encode] → [NetOut] → Network
             ↓          ↓         ↓         ↓         ↓          ↓
          Core 0     Core 1    Core 2    Core 3    Core 4     Core 5
```

## Features

- **Multi-Core**: Each stage runs on a dedicated CPU core
- **Lock-free**: Minimal contention with atomic queues
- **Zero-copy**: Slab allocator for connection state
- **Observable**: Built-in per-stage metrics
- **Cross-platform**: Works on Linux and macOS

## Project Structure

```
ners/
├── ners-core/           # Core kernel and stages
│   ├── src/
│   │   ├── conn.rs      # Connection slab
│   │   ├── queue.rs     # Lock-free ring queue
│   │   ├── net.rs       # TCP I/O
│   │   ├── mux.rs       # I/O multiplexer
│   │   ├── affinity.rs  # CPU core pinning
│   │   ├── executor.rs  # Stage executor
│   │   ├── orchestrator.rs # Multi-stage manager
│   │   ├── stage.rs     # 6-stage pipeline
│   │   ├── handlers.rs  # Route handlers
│   │   └── main.rs      # Multi-threaded entry
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
RUST_LOG=info cargo run --release
# Listening on 0.0.0.0:8080
# Spawns dedicated threads per stage
```

### Test Endpoints

```bash
curl http://localhost:8080/
# Hello, World!

curl http://localhost:8080/api/test
# {"status": "ok", "message": "NERS Phase 1"}
```

### Run Tests

```bash
cargo test
```

## Phase Roadmap

- [x] **Phase 1**: Single-threaded kernel (5k req/sec)
- [x] **Phase 2**: Multi-core with io_uring compatibility (target: 50k+ req/sec)
- [ ] **Phase 3**: Distributed consensus
- [ ] **Phase 4**: AI-native control plane

## License

MIT License - see LICENSE file
