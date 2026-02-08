#!/bin/bash
set -e

DATA_DIR="${DATA_DIR:-/data}"
DAEMON_HOST="${DAEMON_HOST:-daemon.railway.internal}"
DAEMON_ENROLLMENT_PORT="${DAEMON_ENROLLMENT_PORT:-7900}"
DAEMON_RPC_PORT="${DAEMON_RPC_PORT:-7899}"
DAEMON_EVENTS_PORT="${DAEMON_EVENTS_PORT:-7898}"
MCP_GATEWAY_PORT="${MCP_GATEWAY_PORT:-8090}"
MCP_CREATION_POLICY="${MCP_CREATION_POLICY:-open}"
MCP_DATA_DIR="$DATA_DIR/mcp-gateway-data"

mkdir -p "$MCP_DATA_DIR"

echo "Waiting for daemon at $DAEMON_HOST:$DAEMON_ENROLLMENT_PORT..."
for i in $(seq 1 60); do
    if timeout 2 bash -c "echo > /dev/tcp/$DAEMON_HOST/$DAEMON_ENROLLMENT_PORT" 2>/dev/null; then
        echo "Daemon is ready."
        break
    fi
    if [ "$i" -eq 60 ]; then
        echo "Timeout waiting for daemon, starting anyway..."
    fi
    sleep 2
done

exec mcp-proxy \
    --port="$MCP_GATEWAY_PORT" \
    --host=0.0.0.0 \
    --pass-environment \
    -- \
    moor-mcp-host \
        --rpc-address="tcp://$DAEMON_HOST:$DAEMON_RPC_PORT" \
        --events-address="tcp://$DAEMON_HOST:$DAEMON_EVENTS_PORT" \
        --enrollment-address="tcp://$DAEMON_HOST:$DAEMON_ENROLLMENT_PORT" \
        --session-only \
        --creation-policy="$MCP_CREATION_POLICY" \
        --data-dir="$MCP_DATA_DIR"
