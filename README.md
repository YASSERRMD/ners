# NERS - High-Performance Web Server Kernel

A lock-free, multi-core web server with HTTP/1.1, HTTP/2, self-adaptive tuning, and ML-driven policies.

## Architecture

```
Network → [NetIn] → [Parse] → [Route] → [App] → [Encode] → [NetOut] → Network
             ↓          ↓         ↓         ↓         ↓          ↓
          Core 0     Core 1    Core 2    Core 3    Core 4     Core 5
                              ↑
                    ML Policy Bridge + Autotuning
```

## Features

- **Multi-Core**: Each stage runs on a dedicated CPU core
- **HTTP/2 Support**: Frame parsing, HPACK compression, stream multiplexing
- **Lock-Free Sharding**: Per-core slab allocation, NUMA-aware
- **Self-Adaptive**: Automatic queue size and batching tuning
- **ML-Driven**: Feature extraction and learned tuning policies

## Quick Start

```bash
cargo build --release
RUST_LOG=info cargo run --release

curl http://localhost:8080/
curl http://localhost:8080/api/test
```

## Crates

| Crate | Description |
|-------|-------------|
| `ners-core` | Stage pipeline, orchestrator, slab sharding |
| `ners-proto-http` | HTTP/1.1 and HTTP/2 parsing |
| `ners-metrics` | Per-stage metrics collection |
| `ners-ml` | ML policy bridge, feature extraction |

## Phase Roadmap

- [x] **Phase 1**: Single-threaded kernel
- [x] **Phase 2**: Multi-core with io_uring compatibility
- [x] **Phase 3**: Behavioral autotuning
- [x] **Phase 4a**: HTTP/2 support
- [x] **Phase 4b**: Lock-free sharding & NUMA
- [x] **Phase 4c**: ML policy bridge

## License

MIT License
