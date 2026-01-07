# NERS - High-Performance Web Server Kernel

A lock-free, multi-core web server kernel with self-adaptive tuning.

## Architecture

```
Network → [NetIn] → [Parse] → [Route] → [App] → [Encode] → [NetOut] → Network
             ↓          ↓         ↓         ↓         ↓          ↓
          Core 0     Core 1    Core 2    Core 3    Core 4     Core 5
             ↑___________↑__________↑__________↑__________↑__________↑
                              MetricsAnalyzer + TuningEngine
```

## Features

- **Multi-Core**: Each stage runs on a dedicated CPU core
- **Self-Adaptive**: Automatic queue size and batching tuning
- **Lock-free**: Minimal contention with atomic queues
- **Observable**: Built-in per-stage metrics with trend detection
- **Safe**: Rollback mechanism prevents bad configurations

## Quick Start

```bash
# Build
cargo build --release

# Run server
RUST_LOG=info cargo run --release

# Test endpoints
curl http://localhost:8080/
curl http://localhost:8080/api/test
```

## Phase Roadmap

- [x] **Phase 1**: Single-threaded kernel (5k req/sec)
- [x] **Phase 2**: Multi-core with io_uring compatibility
- [x] **Phase 3**: Behavioral autotuning
- [ ] **Phase 4**: AI-native control plane

## Autotuning

NERS automatically monitors:
- Per-stage latency (avg, max)
- Queue depths and overflow events
- Throughput trends

Based on these metrics, it automatically:
- Adjusts queue sizes
- Enables adaptive batching
- Manages backpressure

**Tuning Policies:**
- `ConservativePolicy`: Safe, 10% incremental changes
- `AggressivePolicy`: Fast adaptation, 20% changes

## License

MIT License - see LICENSE file
