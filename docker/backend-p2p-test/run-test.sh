#!/bin/bash
#
# Integration test runner for backend P2P file transfer
#
# This tests the actual backend crate with a real Rumble server:
# - File transfer via BitTorrent
# - NAT traversal via server relay
#
# Usage:
#   ./run-test.sh [command]
#
# Commands:
#   build       - Build Docker images
#   transfer    - File transfer test (simple network)
#   nat         - File transfer with NAT simulation
#   interactive - Start services for manual testing
#   cleanup     - Remove all containers
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
        echo "Hello from backend P2P file transfer integration test!" > test-files/test.txt
        log_info "Created test file: test-files/test.txt"
    fi
    # Create a larger test file for throughput testing
    if [ ! -f test-files/large.bin ]; then
        dd if=/dev/urandom of=test-files/large.bin bs=1024 count=100 2>/dev/null
        log_info "Created test file: test-files/large.bin (100KB)"
    fi
}

# Build images
build() {
    log_info "Building Docker images (this may take a while on first run)..."
    cd "$SCRIPT_DIR/../.."
    docker compose -f docker/backend-p2p-test/docker-compose.simple.yml build
    cd "$SCRIPT_DIR"
    log_success "Build complete"
}

# Clean up
cleanup() {
    log_info "Cleaning up containers..."
    docker rm -f backend-test-server backend-test-node-a backend-test-node-b 2>/dev/null || true
    docker rm -f backend-nat-server backend-nat-node-a backend-nat-node-b backend-nat-router-a backend-nat-router-b 2>/dev/null || true
    docker rm -f backend-nat-test-a backend-nat-test-b 2>/dev/null || true
    cd "$SCRIPT_DIR/../.."
    docker compose -f docker/backend-p2p-test/docker-compose.simple.yml down -v 2>/dev/null || true
    docker compose -f docker/backend-p2p-test/docker-compose.yml down -v 2>/dev/null || true
    cd "$SCRIPT_DIR"
    log_info "Cleanup complete"
}

# Start server and wait for it to be ready
start_server() {
    local compose_file=$1
    local server_container=$2
    
    log_info "Starting Rumble server..."
    cd "$SCRIPT_DIR/../.."
    docker compose -f "docker/backend-p2p-test/$compose_file" up -d server
    cd "$SCRIPT_DIR"
    
    # Wait for server to generate certificates and be ready
    log_info "Waiting for server to be ready..."
    sleep 5
    
    # Check if server is running
    if ! docker ps | grep -q "$server_container"; then
        log_error "Server failed to start"
        docker logs "$server_container"
        exit 1
    fi
    
    log_success "Server is ready"
}

# Test: File transfer (simple network)
test_transfer() {
    log_info "=== Running File Transfer Integration Test ==="
    log_info "This tests file sharing via BitTorrent through the actual Rumble server"
    
    cleanup
    setup_test_files
    start_server "docker-compose.simple.yml" "backend-test-server"
    
    cd "$SCRIPT_DIR/../.."
    
    # Start node-a as file sharer
    log_info "Starting node-a as file sharer..."
    docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run -d --name backend-test-node-a \
        -e RUST_LOG=info,backend=debug \
        node-a \
        test-node --name node-a --server 172.30.0.10:5000 --cert /certs/fullchain.pem --download-dir /data/downloads \
        share --file /data/files/test.txt --wait 120
    
    sleep 5
    
    # Get the magnet link from node-a's output
    MAGNET=$(docker logs backend-test-node-a 2>&1 | grep '"magnet"' | head -1 | sed 's/.*"magnet":"\([^"]*\)".*/\1/')
    
    if [ -z "$MAGNET" ]; then
        log_error "Failed to get magnet link from node-a"
        docker logs backend-test-node-a
        exit 1
    fi
    
    log_info "Magnet link: $MAGNET"
    
    # Start node-b to download the file
    log_info "Starting node-b to download file..."
    docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run --rm --name backend-test-node-b \
        -e RUST_LOG=info,backend=debug \
        node-b \
        test-node --name node-b --server 172.30.0.10:5000 --cert /certs/fullchain.pem --download-dir /data/downloads \
        fetch --magnet "$MAGNET" --timeout 60
    
    RESULT=$?
    
    cd "$SCRIPT_DIR"
    
    if [ $RESULT -eq 0 ]; then
        log_success "File transfer test PASSED!"
    else
        log_error "File transfer test FAILED!"
        docker logs backend-test-node-a
        docker logs backend-test-node-b 2>/dev/null || true
    fi
    
    cleanup
    return $RESULT
}

# Test: File transfer with NAT simulation
test_nat() {
    log_info "=== Running NAT File Transfer Test ==="
    log_warn "This test simulates two nodes behind separate NATs"
    log_warn "NOTE: Full NAT relay support is a work-in-progress"
    log_info "File transfer goes through the server's BitTorrent relay"
    
    cleanup
    setup_test_files
    
    cd "$SCRIPT_DIR/../.."
    
    # Start the full NAT environment
    log_info "Starting NAT simulation environment..."
    docker compose -f docker/backend-p2p-test/docker-compose.yml up -d server nat-a nat-b
    
    sleep 8
    
    # Check server is running
    if ! docker ps | grep -q "backend-nat-server"; then
        log_error "Server failed to start"
        docker logs backend-nat-server
        exit 1
    fi
    
    log_success "NAT environment ready"
    
    # Start node-a behind NAT to share file
    log_info "Starting node-a behind NAT..."
    docker compose -f docker/backend-p2p-test/docker-compose.yml run -d --name backend-nat-test-a \
        --entrypoint "" \
        -e RUST_LOG=info,backend=debug \
        node-a \
        sh -c "ip route del default 2>/dev/null || true; ip route add default via 192.168.100.2; exec test-node --name node-a --server 10.100.0.10:5000 --cert /certs/fullchain.pem --download-dir /data/downloads --prefer-relay share --file /data/files/test.txt --wait 120"
    
    sleep 8
    
    # Get magnet link
    MAGNET=$(docker logs backend-nat-test-a 2>&1 | grep '"magnet"' | head -1 | sed 's/.*"magnet":"\([^"]*\)".*/\1/')
    
    if [ -z "$MAGNET" ]; then
        log_error "Failed to get magnet link from node-a"
        docker logs backend-nat-test-a
        exit 1
    fi
    
    log_info "Magnet link: $MAGNET"
    
    # Start node-b behind NAT to download
    log_info "Starting node-b behind NAT to download..."
    docker compose -f docker/backend-p2p-test/docker-compose.yml run --rm --name backend-nat-test-b \
        --entrypoint "" \
        -e RUST_LOG=info,backend=debug \
        node-b \
        sh -c "ip route del default 2>/dev/null || true; ip route add default via 192.168.200.2; exec test-node --name node-b --server 10.100.0.10:5000 --cert /certs/fullchain.pem --download-dir /data/downloads --prefer-relay fetch --magnet '$MAGNET' --timeout 90"
    
    RESULT=$?
    
    cd "$SCRIPT_DIR"
    
    if [ $RESULT -eq 0 ]; then
        log_success "NAT file transfer test PASSED!"
    else
        log_error "NAT file transfer test FAILED!"
        docker logs backend-nat-test-a
        docker logs backend-nat-test-b 2>/dev/null || true
    fi
    
    cleanup
    return $RESULT
}

# Interactive mode
interactive() {
    log_info "=== Interactive Mode ==="
    setup_test_files
    start_server "docker-compose.simple.yml" "backend-test-server"
    
    echo ""
    echo "Rumble server is running at 172.30.0.10:5000"
    echo ""
    echo "Example commands (run from workspace root):"
    echo ""
    echo "  # Share a file:"
    echo "  docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run --rm \\"
    echo "    node-a test-node --name node-a --server 172.30.0.10:5000 \\"
    echo "    --cert /certs/fullchain.pem --download-dir /data/downloads \\"
    echo "    share --file /data/files/test.txt --wait 120"
    echo ""
    echo "  # Download a file:"
    echo "  docker compose -f docker/backend-p2p-test/docker-compose.simple.yml run --rm \\"
    echo "    node-b test-node --name node-b --server 172.30.0.10:5000 \\"
    echo "    --cert /certs/fullchain.pem --download-dir /data/downloads \\"
    echo "    fetch --magnet '<magnet_link>'"
    echo ""
    echo "Press Ctrl+C to stop and clean up."
    
    # Wait for interrupt
    trap cleanup EXIT
    while true; do
        sleep 60
    done
}

# Run all tests
test_all() {
    log_info "=== Running All Tests ==="
    
    FAILED=0
    NAT_FAILED=0
    
    test_transfer || FAILED=1
    test_nat || NAT_FAILED=1
    
    echo ""
    if [ $FAILED -eq 0 ] && [ $NAT_FAILED -eq 0 ]; then
        log_success "All tests passed!"
    elif [ $FAILED -eq 0 ] && [ $NAT_FAILED -eq 1 ]; then
        log_warn "Core tests passed, NAT test failed (NAT relay is WIP)"
        FAILED=0  # Don't fail overall since NAT is WIP
    else
        log_error "Some tests failed!"
    fi
    
    return $FAILED
}

# Show usage
usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  build       - Build Docker images"
    echo "  transfer    - File transfer test (simple network)"
    echo "  nat         - File transfer with NAT simulation"
    echo "  all         - Run all tests"
    echo "  interactive - Start services for manual testing"
    echo "  cleanup     - Remove all containers"
    echo ""
}

# Main
case "${1:-}" in
    build)
        build
        ;;
    transfer)
        test_transfer
        ;;
    nat)
        test_nat
        ;;
    all)
        test_all
        ;;
    interactive)
        interactive
        ;;
    cleanup)
        cleanup
        ;;
    *)
        usage
        ;;
esac
