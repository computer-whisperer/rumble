# P2P NAT Traversal Test Environment

Docker-based test environment for testing P2P file transfer with NAT traversal.

## Connection Priority

The P2P system uses this connection priority:

1. **IPv6 direct** - No NAT traversal needed (highest priority)
2. **QUIC relay** - UDP-based, better for potential future hole punching
3. **TCP relay** - Reliable fallback (always works)

## Supported Transports

- **TCP** - Traditional reliable transport
- **QUIC** - UDP-based transport with built-in encryption (better for NAT traversal)
- **IPv6** - Dual-stack support for direct connectivity when available

## Quick Start

```bash
cd docker/p2p-test

# Run the full NAT holepunch test
./run-test.sh holepunch

# Clean up containers when done
./run-test.sh cleanup
```

## Network Topology

The test simulates two peers behind separate NATs communicating via a relay:

```
                     10.99.0.0/24 (public)
                            |
                     +------+------+
                     |   relay     |
                     | 10.99.0.10  |
                     +------+------+
                            |
              +-------------+-------------+
              |                           |
        +-----+-----+               +-----+-----+
        |   nat-a   |               |   nat-b   |
        | 10.99.0.20|               | 10.99.0.30|
        | (MASQ)    |               | (MASQ)    |
        +-----+-----+               +-----+-----+
              |                           |
       10.99.1.0/24                10.99.2.0/24
       (private-a)                 (private-b)
              |                           |
        +-----+-----+               +-----+-----+
        |  node-a   |               |  node-b   |
        |10.99.1.100|               |10.99.2.100|
        | (sharer)  |               | (fetcher) |
        +-----------+               +-----------+
```

## Test Commands

| Command | Description |
|---------|-------------|
| `./run-test.sh holepunch` | Full NAT test - both peers behind NAT, file transfer via relay |
| `./run-test.sh relay` | Basic relay test with direct connections |
| `./run-test.sh cleanup` | Remove all test containers and networks |

## What the Holepunch Test Does

1. Starts relay server on public network (10.99.0.10:4001)
2. Starts two NAT routers with iptables MASQUERADE
3. Starts node-a behind NAT-A, shares a test file via relay circuit
4. Starts node-b behind NAT-B, fetches file via relay circuit address
5. Verifies file content matches

## Expected Output (Success)

```
[SUCCESS] Relay started with Peer ID: 12D3KooW...
[SUCCESS] NAT routers ready
[SUCCESS] Node A has relay circuit reservation
...
========================================
FILE RECEIVED
========================================
Name: test.txt
Size: 76 bytes
Content: Hello from P2P file transfer test!
...
[SUCCESS] File transfer succeeded via relay!
```

## Troubleshooting

### NAT routers fail to initialize
The NAT routers need time to install iptables (~20-30s). The script polls for up to 60 seconds. If it still fails:
```bash
docker logs p2p-test-nat-a-1
```

### Connection timeouts
Verify NAT routing is working:
```bash
# Start the environment manually
docker compose up -d relay nat-a nat-b
sleep 25

# Check NAT-A iptables
docker exec p2p-test-nat-a-1 iptables -t nat -L -v

# Test connectivity from a node
docker compose run --rm node-a sh -c "
  ip route del default 2>/dev/null || true
  ip route add default via 10.99.1.2
  ping -c 2 10.99.0.10
"
```

### Why DCUtR is Disabled

DCUtR (Direct Connection Upgrade through Relay) is **intentionally disabled** because:

1. **Aggressive connection closure**: In libp2p 0.52, DCUtR closes relay connections when hole punching fails with "NoAddresses" error
2. **Symmetric NAT incompatibility**: When both peers are behind symmetric NAT, there are no routable addresses to exchange
3. **Breaks relay fallback**: The connection closure prevents relay-based communication from working

**Current approach**: Use relay tunneling as the reliable fallback. True hole punching for symmetric NAT would require:
- A STUN server to discover external addresses
- Custom signaling to exchange candidate addresses
- Manual UDP hole punching without libp2p's DCUtR

## Key Implementation Notes

1. **Dynamic interface detection**: NAT routers detect public/private interfaces by IP pattern (`grep 10.99.0` vs `grep 10.99.1`), not hardcoded eth0/eth1 (Docker assigns interfaces in unpredictable order)

2. **NET_ADMIN capability**: Required for both NAT routers (iptables) and nodes (ip route)

3. **Internal networks**: private-a and private-b are marked `internal: true` to prevent direct external access

4. **Sysctl for IP forwarding**: Use `sysctls: net.ipv4.ip_forward=1` instead of `echo 1 > /proc/sys/net/ipv4/ip_forward` (avoids permission errors)

5. **No DCUtR in test nodes**: DCUtR is removed from test_node.rs because it closes relay connections when hole punching fails (which always happens with symmetric NAT)

## Files

| File | Purpose |
|------|---------|
| `docker-compose.yml` | Network topology and container definitions |
| `run-test.sh` | Test automation script |
| `Dockerfile` | Builds relay-server and test-node binaries |
| `src/relay_server.rs` | Relay server with Circuit Relay v2 |
| `src/test_node.rs` | Test node for file sharing/fetching (no DCUtR) |
| `test-files/test.txt` | Sample file for transfer tests |

## Test Binaries

### relay-server

A libp2p relay server with Circuit Relay v2:

```bash
relay-server --port 4001 --bind 0.0.0.0
```

### test-node

A test client that can share or fetch files via relay:

**Share a file:**
```bash
test-node --name my-node --relay <RELAY_ADDR> --relay-listen share --file /path/to/file
```

**Fetch a file:**
```bash
test-node --name fetcher --relay <RELAY_ADDR> --relay-listen fetch \
  --target <CIRCUIT_ADDR> --file-id <FILE_ID>
```

## Manual Testing

```bash
# Build images
docker compose build

# Start relay
docker compose up -d relay
sleep 5
RELAY_ID=$(docker logs p2p-test-relay-1 2>&1 | grep "Peer ID:" | awk '{print $NF}')
echo "Relay ID: $RELAY_ID"

# Start NAT routers and wait for iptables setup
docker compose up -d nat-a nat-b
sleep 25

# Start node-a as file sharer
docker compose run -d --name test-node-a \
  -e RUST_LOG=info,libp2p=debug \
  --entrypoint sh node-a \
  -c "ip route del default 2>/dev/null; ip route add default via 10.99.1.2; \
      exec test-node --name node-a \
        --relay /ip4/10.99.0.10/tcp/4001/p2p/$RELAY_ID \
        --relay-listen share --file /data/test.txt"

sleep 5
NODE_A_ID=$(docker logs test-node-a 2>&1 | grep "Peer ID:" | awk '{print $NF}')
FILE_ID=$(docker logs test-node-a 2>&1 | grep "File ID:" | awk '{print $NF}')
echo "Node A ID: $NODE_A_ID"
echo "File ID: $FILE_ID"

# Wait for relay reservation
sleep 10

# Start node-b to fetch (this will print the file contents)
docker compose run --name test-node-b \
  -e RUST_LOG=info,libp2p=debug \
  --entrypoint sh node-b \
  -c "ip route del default 2>/dev/null; ip route add default via 10.99.2.2; \
      exec test-node --name node-b \
        --relay /ip4/10.99.0.10/tcp/4001/p2p/$RELAY_ID \
        --relay-listen fetch \
        --target /ip4/10.99.0.10/tcp/4001/p2p/$RELAY_ID/p2p-circuit/p2p/$NODE_A_ID \
        --file-id $FILE_ID"

# Cleanup
docker rm -f test-node-a test-node-b
docker compose down -v
```

## Protocol Details

- **Transports**:
  - TCP with Noise encryption and Yamux multiplexing
  - QUIC with built-in TLS 1.3 encryption (UDP-based)
- **Relay**: libp2p Circuit Relay v2
- **File Transfer**: Custom request-response protocol (`/rumble/file/1.0.0`)
- **Addressing**: Dual-stack IPv4/IPv6 support

### File Request: 32-byte BLAKE3 hash

### File Response:
```
[1 byte: ok flag]
[2 bytes: name length (big-endian)]
[8 bytes: data length (big-endian)]
[N bytes: file name]
[M bytes: file data]
```
