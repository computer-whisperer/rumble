# P2P NAT Traversal Testing

Docker-based test environment for P2P connectivity with NAT simulation using libp2p.

## Location

```
docker/p2p-test/
├── src/
│   ├── test_node.rs    # Test node with DCUtR hole punching
│   └── relay_server.rs # Relay server for NAT traversal
├── run-test.sh         # Test runner script
├── docker-compose.yml  # Full NAT simulation environment
└── docker-compose.simple.yml  # Simple direct/relay tests
```

## Running Tests

```bash
cd docker/p2p-test

# Build Docker images
./run-test.sh build

# Run specific tests
./run-test.sh direct      # Direct connection (no NAT)
./run-test.sh relay       # Connection via relay circuit
./run-test.sh holepunch   # NAT hole punching with DCUtR

# Run all tests
./run-test.sh all

# Clean up containers
./run-test.sh cleanup

# Interactive mode (start relay, get instructions)
./run-test.sh interactive
```

## Network Topology (holepunch test)

```
                    ┌─────────────────┐
                    │     Relay       │
                    │   10.99.0.10    │
                    └────────┬────────┘
                             │ public-net (10.99.0.0/24)
            ┌────────────────┴────────────────┐
            │                                 │
     ┌──────┴──────┐                   ┌──────┴──────┐
     │   NAT-A     │                   │   NAT-B     │
     │ 10.99.0.20  │                   │ 10.99.0.30  │
     │ 10.99.1.2   │                   │ 10.99.2.2   │
     └──────┬──────┘                   └──────┴──────┘
            │ private-a (10.99.1.0/24)        │ private-b (10.99.2.0/24)
            │                                 │
     ┌──────┴──────┐                   ┌──────┴──────┐
     │   Node-A    │                   │   Node-B    │
     │ 10.99.1.10  │                   │ 10.99.2.10  │
     │ (sharer)    │                   │ (fetcher)   │
     └─────────────┘                   └─────────────┘
```

## Test Descriptions

1. **direct**: Node-A shares a file, Node-B connects directly (no NAT)
2. **relay**: Node-A listens via relay circuit, Node-B fetches through relay
3. **holepunch**: Both nodes behind NAT, DCUtR attempts hole punch, falls back to relay

## Expected Behavior

- With symmetric NAT (iptables MASQUERADE), hole punching will fail with "Connection refused"
- This is expected - symmetric NAT creates destination-specific port mappings
- The test succeeds via relay fallback: `FILE RECEIVED` with `HOLEPUNCH_FAILED`
- Successful hole punch shows: `HOLEPUNCH_SUCCESS` (requires endpoint-independent NAT)
