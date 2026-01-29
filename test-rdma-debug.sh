#!/bin/bash
# Test script with debug logging enabled

echo "This script helps debug RDMA GET operations"
echo ""
echo "Usage:"
echo "  On Server machine: $0 server"
echo "  On Client machine: $0 client <server-ip>"
echo ""

if [ "$1" == "server" ]; then
    echo "Starting server with debug logging..."
    RUST_LOG=debug ./run-with-rdma.sh ./target/release/kv-server \
        --listen-addr "0.0.0.0:50051" \
        --log-level debug

elif [ "$1" == "client" ]; then
    if [ -z "$2" ]; then
        echo "Error: Please provide server IP"
        echo "Example: $0 client 10.0.148.107"
        exit 1
    fi

    SERVER_IP="$2"
    echo "Testing against server at $SERVER_IP:50051"
    echo ""

    # PUT test
    echo "=== PUT Test ==="
    RUST_LOG=debug ./run-with-rdma.sh ./target/release/kv-client \
        --server-addr "http://$SERVER_IP:50051" \
        --log-level debug \
        put testkey "Hello RDMA World" 2>&1 | grep -E "INFO|DEBUG|Error|OK"

    echo ""
    echo "=== GET Test (with full debug output) ==="
    RUST_LOG=debug ./run-with-rdma.sh ./target/release/kv-client \
        --server-addr "http://$SERVER_IP:50051" \
        --log-level debug \
        get testkey

else
    echo "Error: Invalid command"
    echo "Use: $0 server   OR   $0 client <server-ip>"
    exit 1
fi
