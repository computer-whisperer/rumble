# Backend P2P Integration Test Environment

This directory contains a Docker-based integration test environment for testing the **backend crate's** P2P file transfer functionality with the **actual Rumble server**.

## What This Tests

1. **File transfer via BitTorrent** - Using `backend::BackendHandle` to share and download files
2. **NAT traversal via server relay** - Server's BitTorrent relay service for NAT'd clients
3. **Full client-server integration** - Real QUIC connections, authentication, and protocol

## Architecture

```
┌─────────────────────────────────────────┐
│       test-network (172.30.0.0/24)      │
│                                         │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  │
│  │ server  │  │ node-a  │  │ node-b  │  │
│  │ Rumble  │  │ sharer  │  │ fetcher │  │
│  │ .0.10   │  │ .0.20   │  │ .0.30   │  │
│  └─────────┘  └─────────┘  └─────────┘  │
└─────────────────────────────────────────┘
```

For NAT simulation, see `docker-compose.yml` which creates separate private networks behind simulated NAT routers.

## Quick Start

### 1. Build the images

```bash
./run-test.sh build
```

This builds the actual Rumble server and test node binaries.

### 2. Run tests

```bash
# Run all tests
./run-test.sh all

# Run individual tests
./run-test.sh transfer   # File transfer (simple network)
./run-test.sh nat        # File transfer with NAT simulation
```

### 3. Interactive testing

```bash
./run-test.sh interactive
```

This starts the Rumble server and provides instructions for manually running test nodes.

### 4. Cleanup

```bash
./run-test.sh cleanup
```

## Test Descriptions

### File Transfer Test (`transfer`)

Simple network test:
1. Rumble server starts and generates TLS certificates
2. Node A connects to server and shares a file via BitTorrent
3. Node A receives a magnet link
4. Node B connects to server and downloads using the magnet link
5. Verifies transfer completion

### NAT Transfer Test (`nat`)

Full NAT simulation:
1. Two NAT routers are created with iptables masquerading
2. Node A and Node B are placed behind separate NATs
3. Both connect to the public Rumble server
4. File transfer uses the server's BitTorrent relay service

## Manual Testing

After starting with `./run-test.sh interactive`:

```bash
# Share a file (from workspace root)
docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run --rm \
    node-a test-node --name node-a --server 172.30.0.10:5000 \
    --cert /certs/fullchain.pem --download-dir /data/downloads \
    share --file /data/files/test.txt --wait 120

# Download a file
docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run --rm \
    node-b test-node --name node-b --server 172.30.0.10:5000 \
    --cert /certs/fullchain.pem --download-dir /data/downloads \
    fetch --magnet '<magnet_link>'
```

## Test Node Commands

The `test-node` binary supports:

- `share` - Share a file via BitTorrent
  - `--file <path>` - File to share
  - `--wait <secs>` - How long to seed (0 = forever)

- `fetch` - Download a file using magnet link
  - `--magnet <link>` - Magnet link
  - `--timeout <secs>` - Download timeout

- `wait` - Just connect and wait
  - `--duration <secs>` - How long (0 = forever)

- `info` - Print node info and exit

## Environment Variables

- `RUST_LOG` - Logging level (e.g., `info,backend=debug,server=debug`)

## Files

```
docker/backend-p2p-test/
├── Cargo.toml              # Test node crate (depends on backend)
├── Dockerfile              # Builds server + test-node binaries
├── docker-compose.simple.yml  # Simple bridge network
├── docker-compose.yml      # Full NAT simulation
├── run-test.sh             # Test runner script
├── README.md               # This file
├── src/
│   └── test_node.rs        # Test node using BackendHandle
└── test-files/             # Test data
```

## Troubleshooting

### Build fails

Ensure Docker has enough resources and the workspace is complete:

```bash
cd /path/to/rumble
docker compose -f docker/backend-p2p-test/docker-compose.simple.yml build --no-cache
```

### Connection timeouts

Check that:
1. Server is running: `docker logs backend-test-server`
2. Certificates were generated: check for `fullchain.pem` in server output
3. Network connectivity between containers

### Transfer fails

Check logs:
```bash
docker logs backend-test-server
docker logs backend-test-node-a
docker logs backend-test-node-b
```
