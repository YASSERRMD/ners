# Build stage
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for caching
COPY Cargo.toml ./
COPY ners-core/Cargo.toml ners-core/
COPY ners-proto-http/Cargo.toml ners-proto-http/
COPY ners-metrics/Cargo.toml ners-metrics/
COPY ners-ml/Cargo.toml ners-ml/

# Create dummy files for dependency caching
RUN mkdir -p ners-core/src ners-proto-http/src ners-metrics/src ners-ml/src && \
    echo "fn main() {}" > ners-core/src/main.rs && \
    echo "pub fn lib() {}" > ners-core/src/lib.rs && \
    echo "pub fn lib() {}" > ners-proto-http/src/lib.rs && \
    echo "pub fn lib() {}" > ners-metrics/src/lib.rs && \
    echo "pub fn lib() {}" > ners-ml/src/lib.rs

# Build dependencies only
RUN cargo build --release || true

# Copy actual source code
COPY ners-core/src ners-core/src
COPY ners-proto-http/src ners-proto-http/src
COPY ners-metrics/src ners-metrics/src
COPY ners-ml/src ners-ml/src
COPY ners-core/benches ners-core/benches

# Touch to invalidate cache
RUN touch ners-core/src/main.rs

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/ners /usr/local/bin/ners

# Copy assets
COPY assets /app/assets

# Create non-root user
RUN useradd -r -s /bin/false ners && \
    chown -R ners:ners /app

USER ners

# Expose port
EXPOSE 8080

# Environment
ENV RUST_LOG=info
ENV NERS_PORT=8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run
CMD ["ners"]
