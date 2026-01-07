# NERS - High-Performance Web Server Kernel

A lock-free, multi-core web server with HTTP/1.1, HTTP/2, and self-adaptive tuning.

## Architecture

```
Network → [NetIn] → [Parse] → [Route] → [App] → [Encode] → [NetOut] → Network
             ↓          ↓         ↓         ↓         ↓          ↓
          Core 0     Core 1    Core 2    Core 3    Core 4     Core 5
```

## Features

- **Multi-Core**: Each stage runs on a dedicated CPU core
- **HTTP/2 Support**: Frame parsing, HPACK compression, stream multiplexing
- **Self-Adaptive**: Automatic queue size and batching tuning
- **Lock-free**: Minimal contention with atomic queues

## Quick Start

```bash
cargo build --release
RUST_LOG=info cargo run --release

curl http://localhost:8080/
curl http://localhost:8080/api/test
```

## HTTP/2 Support (Phase 4a)

- **Frame Types**: DATA, HEADERS, SETTINGS, WINDOW_UPDATE, etc.
- **HPACK**: Header compression with static/dynamic tables
- **Stream Multiplexing**: 100+ concurrent streams per connection
- **Flow Control**: Per-stream and connection window management

## Phase Roadmap

- [x] **Phase 1**: Single-threaded kernel
- [x] **Phase 2**: Multi-core with io_uring compatibility
- [x] **Phase 3**: Behavioral autotuning
- [x] **Phase 4a**: HTTP/2 support

## License

MIT License
