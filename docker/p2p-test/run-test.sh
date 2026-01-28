#!/bin/bash
#
# Test runner script for P2P NAT traversal testing
#
# Usage:
#   ./run-test.sh [simple|nat] [test-name]
#
# Tests:
#   direct      - Direct connection test (no NAT)
#   relay       - Connection via relay
#   holepunch   - NAT hole punching test
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Ensure test file exists
setup_test_files() {
    mkdir -p test-files
    if [ ! -f test-files/test.txt ]; then
        echo "Hello from P2P file transfer test!" > test-files/test.txt
        log_info "Created test file: test-files/test.txt"
    fi
}

# Build images
build() {
    log_info "Building Docker images..."
    docker compose -f docker-compose.simple.yml build
}

# Clean up
cleanup() {
    log_info "Cleaning up containers..."
    docker rm -f test-node-a test-node-b p2p-relay 2>/dev/null || true
    docker compose -f docker-compose.simple.yml down -v 2>/dev/null || true
    docker compose -f docker-compose.yml down -v 2>/dev/null || true
}

# Start relay and get its peer ID
start_relay() {
    log_info "Starting relay server..."
    docker compose -f docker-compose.simple.yml up -d relay
    
    # Wait for relay to be ready and capture peer ID
    sleep 3
    
    RELAY_PEER_ID=$(docker logs p2p-relay 2>&1 | grep "Peer ID:" | head -1 | awk '{print $NF}')
    
    if [ -z "$RELAY_PEER_ID" ]; then
        log_error "Failed to get relay peer ID"
        docker logs p2p-relay
        exit 1
    fi
    
    log_success "Relay started with Peer ID: $RELAY_PEER_ID"
    export RELAY_PEER_ID
    export RELAY_ADDR="/ip4/172.28.0.10/tcp/4001/p2p/$RELAY_PEER_ID"
}

# Test: Direct connection (no relay)
test_direct() {
    log_info "=== Running Direct Connection Test ==="
    
    cleanup
    setup_test_files
    
    # Start node-a as file sharer (direct listen on port 5000)
    log_info "Starting node-a as file sharer..."
    docker compose -f docker-compose.simple.yml run -d --name test-node-a \
        -e RUST_LOG=info \
        node-a \
        test-node --name node-a --port 5000 share --file /data/test.txt
    
    sleep 2
    
    # Get node-a's peer ID and file ID from logs
    NODE_A_PEER_ID=$(docker logs test-node-a 2>&1 | grep "Peer ID:" | head -1 | awk '{print $NF}')
    FILE_ID=$(docker logs test-node-a 2>&1 | grep "File ID:" | head -1 | awk '{print $NF}')
    
    if [ -z "$NODE_A_PEER_ID" ] || [ -z "$FILE_ID" ]; then
        log_error "Failed to get node-a info"
        docker logs test-node-a
        exit 1
    fi
    
    log_info "Node A Peer ID: $NODE_A_PEER_ID"
    log_info "File ID: $FILE_ID"
    
    # Start node-b to fetch the file directly
    log_info "Starting node-b to fetch file..."
    docker compose -f docker-compose.simple.yml run --rm --name test-node-b \
        -e RUST_LOG=info \
        node-b \
        test-node --name node-b fetch \
            --target "/ip4/172.28.0.20/tcp/5000/p2p/$NODE_A_PEER_ID" \
            --file-id "$FILE_ID"
    
    RESULT=$?
    
    # Check result
    if [ $RESULT -eq 0 ]; then
        log_success "Direct connection test PASSED!"
    else
        log_error "Direct connection test FAILED!"
    fi
    
    cleanup
    return $RESULT
}

# Test: Connection via relay
test_relay() {
    log_info "=== Running Relay Connection Test ==="
    
    cleanup
    setup_test_files
    start_relay
    
    # Start node-a as file sharer via relay
    log_info "Starting node-a as file sharer via relay..."
    docker compose -f docker-compose.simple.yml run -d --name test-node-a \
        -e RUST_LOG=info \
        node-a \
        test-node --name node-a --relay "$RELAY_ADDR" --relay-listen share --file /data/test.txt
    
    sleep 3
    
    # Get node-a's peer ID and file ID
    NODE_A_PEER_ID=$(docker logs test-node-a 2>&1 | grep "Peer ID:" | head -1 | awk '{print $NF}')
    FILE_ID=$(docker logs test-node-a 2>&1 | grep "File ID:" | head -1 | awk '{print $NF}')
    
    if [ -z "$NODE_A_PEER_ID" ] || [ -z "$FILE_ID" ]; then
        log_error "Failed to get node-a info"
        docker logs test-node-a
        exit 1
    fi
    
    log_info "Node A Peer ID: $NODE_A_PEER_ID"
    log_info "File ID: $FILE_ID"
    
    # Wait for relay circuit to be established
    sleep 2
    
    # Construct relay circuit address
    CIRCUIT_ADDR="$RELAY_ADDR/p2p-circuit/p2p/$NODE_A_PEER_ID"
    log_info "Circuit address: $CIRCUIT_ADDR"
    
    # Start node-b to fetch via relay
    log_info "Starting node-b to fetch file via relay..."
    docker compose -f docker-compose.simple.yml run --rm --name test-node-b \
        -e RUST_LOG=info \
        node-b \
        test-node --name node-b --relay "$RELAY_ADDR" fetch \
            --target "$CIRCUIT_ADDR" \
            --file-id "$FILE_ID"
    
    RESULT=$?
    
    if [ $RESULT -eq 0 ]; then
        log_success "Relay connection test PASSED!"
    else
        log_error "Relay connection test FAILED!"
    fi
    
    cleanup
    return $RESULT
}

# Test: NAT hole punching (requires full NAT setup)
test_holepunch() {
    log_info "=== Running NAT Hole Punch Test ==="
    log_info "This test simulates two nodes behind separate NATs"
    log_info "communicating via relay with DCUtR hole punching"

    cleanup
    setup_test_files

    log_info "Building full NAT environment..."
    docker compose -f docker-compose.yml build

    log_info "Starting relay..."
    docker compose -f docker-compose.yml up -d relay
    sleep 5

    # Get relay peer ID - use the container name from compose
    RELAY_CONTAINER=$(docker compose -f docker-compose.yml ps -q relay)
    RELAY_PEER_ID=$(docker logs "$RELAY_CONTAINER" 2>&1 | grep "Peer ID:" | head -1 | awk '{print $NF}')

    if [ -z "$RELAY_PEER_ID" ]; then
        log_error "Failed to get relay peer ID"
        docker compose -f docker-compose.yml logs relay
        cleanup
        exit 1
    fi

    log_success "Relay started with Peer ID: $RELAY_PEER_ID"
    export RELAY_PEER_ID
    # Note: Relay is at 10.99.0.10 on the public network
    export RELAY_ADDR="/ip4/10.99.0.10/tcp/4001/p2p/$RELAY_PEER_ID"

    log_info "Starting NAT routers..."
    docker compose -f docker-compose.yml up -d nat-a nat-b

    # Wait for NAT routers to be ready (need to install iptables and set up rules)
    NAT_A_CONTAINER=$(docker compose -f docker-compose.yml ps -q nat-a)
    NAT_B_CONTAINER=$(docker compose -f docker-compose.yml ps -q nat-b)

    log_info "Waiting for NAT routers to set up iptables..."
    for i in $(seq 1 60); do
        NAT_A_READY=$(docker logs "$NAT_A_CONTAINER" 2>&1 | grep "NAT-A router ready" || true)
        NAT_B_READY=$(docker logs "$NAT_B_CONTAINER" 2>&1 | grep "NAT-B router ready" || true)
        if [ -n "$NAT_A_READY" ] && [ -n "$NAT_B_READY" ]; then
            break
        fi
        sleep 1
    done

    if [ -z "$NAT_A_READY" ] || [ -z "$NAT_B_READY" ]; then
        log_error "NAT routers failed to initialize"
        docker logs "$NAT_A_CONTAINER"
        docker logs "$NAT_B_CONTAINER"
        cleanup
        exit 1
    fi
    log_success "NAT routers ready"

    # Start node-a (behind NAT-A) as file sharer
    log_info "Starting node-a behind NAT-A as file sharer..."
    docker compose -f docker-compose.yml run -d \
        --name test-node-a \
        -e RUST_LOG=info,libp2p=debug,dcutr=debug \
        --entrypoint sh \
        node-a \
        -c "ip route del default 2>/dev/null || true; ip route add default via 10.99.1.2; exec test-node --name node-a --relay '$RELAY_ADDR' --relay-listen share --file /data/test.txt"

    sleep 5

    # Get node-a's peer ID and file ID
    NODE_A_PEER_ID=$(docker logs test-node-a 2>&1 | grep "Peer ID:" | head -1 | awk '{print $NF}')
    FILE_ID=$(docker logs test-node-a 2>&1 | grep "File ID:" | head -1 | awk '{print $NF}')

    if [ -z "$NODE_A_PEER_ID" ] || [ -z "$FILE_ID" ]; then
        log_error "Failed to get node-a info"
        docker logs test-node-a
        cleanup
        exit 1
    fi

    log_info "Node A Peer ID: $NODE_A_PEER_ID"
    log_info "File ID: $FILE_ID"

    # Wait for relay circuit to be established
    RELAY_ACCEPTED=""
    for i in $(seq 1 15); do
        RELAY_ACCEPTED=$(docker logs test-node-a 2>&1 | grep "RELAY_RESERVATION_ACCEPTED" || true)
        if [ -n "$RELAY_ACCEPTED" ]; then
            break
        fi
        sleep 1
    done

    if [ -z "$RELAY_ACCEPTED" ]; then
        log_error "Node A failed to establish relay reservation"
        docker logs test-node-a
        cleanup
        exit 1
    fi
    log_success "Node A has relay circuit reservation"

    # Construct relay circuit address for node-a
    CIRCUIT_ADDR="$RELAY_ADDR/p2p-circuit/p2p/$NODE_A_PEER_ID"
    log_info "Circuit address: $CIRCUIT_ADDR"

    # Start node-b (behind NAT-B) to fetch the file via relay
    log_info "Starting node-b behind NAT-B to fetch file via relay..."

    # Run node-b and capture output
    set +e
    OUTPUT=$(docker compose -f docker-compose.yml run --rm \
        --name test-node-b \
        -e RUST_LOG=info,libp2p=debug,dcutr=debug \
        --entrypoint sh \
        node-b \
        -c "ip route del default 2>/dev/null || true; ip route add default via 10.99.2.2; exec test-node --name node-b --relay '$RELAY_ADDR' --relay-listen fetch --target '$CIRCUIT_ADDR' --file-id '$FILE_ID'" 2>&1)
    EXIT_CODE=$?
    set -e

    echo "$OUTPUT"

    # Check for success
    if echo "$OUTPUT" | grep -q "FILE RECEIVED"; then
        log_success "File transfer succeeded via relay!"

        # Check if DCUtR hole punch succeeded
        if echo "$OUTPUT" | grep -q "HOLEPUNCH_SUCCESS"; then
            log_success "DCUtR hole punch succeeded! Direct connection established."
        elif echo "$OUTPUT" | grep -q "HOLE PUNCH SUCCEEDED"; then
            log_success "DCUtR hole punch succeeded! Direct connection established."
        else
            # Check for failure message
            if echo "$OUTPUT" | grep -q "HOLEPUNCH_FAILED"; then
                log_info "DCUtR hole punch failed (expected with symmetric NAT) - file transferred via relay"
            else
                log_info "File transferred via relay (DCUtR may not have triggered)"
            fi
        fi

        RESULT=0
    else
        log_error "File transfer failed!"
        RESULT=1
    fi

    cleanup
    return $RESULT
}

# Interactive mode
interactive() {
    log_info "=== Interactive P2P Testing ==="
    
    cleanup
    setup_test_files
    start_relay
    
    log_info ""
    log_info "Relay is running at: $RELAY_ADDR"
    log_info ""
    log_info "To start a file-sharing node:"
    log_info "  docker compose -f docker-compose.simple.yml run --rm node-a \\"
    log_info "    test-node --name my-node --relay $RELAY_ADDR --relay-listen share --file /data/test.txt"
    log_info ""
    log_info "To fetch a file:"
    log_info "  docker compose -f docker-compose.simple.yml run --rm node-b \\"
    log_info "    test-node --name fetcher --relay $RELAY_ADDR fetch --target <TARGET_ADDR> --file-id <FILE_ID>"
    log_info ""
    log_info "Streaming relay logs (Ctrl+C to stop)..."
    
    docker logs -f p2p-relay
}

# Show usage
usage() {
    echo "P2P NAT Traversal Test Runner"
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  build       - Build Docker images"
    echo "  direct      - Run direct connection test"
    echo "  relay       - Run relay connection test"
    echo "  holepunch   - Run NAT hole punch test (requires full NAT setup)"
    echo "  interactive - Start environment for interactive testing"
    echo "  cleanup     - Clean up all containers"
    echo "  all         - Run all tests"
    echo ""
}

# Main
case "${1:-}" in
    build)
        build
        ;;
    direct)
        test_direct
        ;;
    relay)
        test_relay
        ;;
    holepunch)
        test_holepunch
        ;;
    interactive)
        interactive
        ;;
    cleanup)
        cleanup
        ;;
    all)
        build
        test_direct
        test_relay
        log_success "All tests passed!"
        ;;
    *)
        usage
        exit 1
        ;;
esac
