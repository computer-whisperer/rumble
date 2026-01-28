# Multi-stage build for P2P NAT testing binaries
FROM rust:1.85-bookworm AS builder

# Install dependencies for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy only the p2p-test crate (standalone, no workspace dependencies)
COPY Cargo.toml ./Cargo.toml
COPY src ./src

# Build the test binaries
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    iproute2 \
    iptables \
    net-tools \
    iputils-ping \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/relay-server /usr/local/bin/
COPY --from=builder /app/target/release/test-node /usr/local/bin/

WORKDIR /app

# Default command
CMD ["test-node"]
