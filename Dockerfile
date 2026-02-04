# Ouroboros Node - Docker Image
# Multi-stage build for smaller final image

# Stage 1: Build
FROM rust:1.75-bookworm as builder

WORKDIR /build

# Install dependencies
RUN apt-get update && apt-get install -y \
    clang \
    libclang-dev \
    cmake \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY ouro_dag/ ./ouro_dag/

# Build release binary
WORKDIR /build/ouro_dag
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash ouroboros

# Create data directory
RUN mkdir -p /data && chown ouroboros:ouroboros /data

# Copy binary from builder
COPY --from=builder /build/ouro_dag/target/release/ouro_dag /usr/local/bin/ouro-node

# Set permissions
RUN chmod +x /usr/local/bin/ouro-node

# Switch to non-root user
USER ouroboros
WORKDIR /home/ouroboros

# Default environment variables
ENV ROCKSDB_PATH=/data \
    API_ADDR=0.0.0.0:8000 \
    LISTEN_ADDR=0.0.0.0:9000 \
    RUST_LOG=info \
    STORAGE_MODE=full

# Expose ports
# 8000 = API (HTTP)
# 9000 = P2P
EXPOSE 8000 9000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Data volume
VOLUME ["/data"]

# Default command
ENTRYPOINT ["ouro-node"]
CMD ["start"]
