# Multi-stage build for backend P2P test binaries
# This builds within the workspace context to access the backend crate
# Using nightly Rust because rqbit fork uses unstable let_chains feature
FROM rustlang/rust:nightly-bookworm AS builder

# Install dependencies for building (including audio/opus/ssl)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    clang \
    libasound2-dev \
    libopus-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the entire workspace since we need all crates
COPY Cargo.toml ./Cargo.toml
COPY crates ./crates
COPY docker/backend-p2p-test ./docker/backend-p2p-test

# Build the server binary from crates/server
WORKDIR /app
RUN cargo build --release --bin server

# Build the test node from docker/backend-p2p-test
WORKDIR /app/docker/backend-p2p-test
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libasound2 \
    libopus0 \
    iproute2 \
    iptables \
    net-tools \
    iputils-ping \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/server /usr/local/bin/rumble-server
COPY --from=builder /app/docker/backend-p2p-test/target/release/test-node /usr/local/bin/

WORKDIR /app

# Default command
CMD ["test-node"]
