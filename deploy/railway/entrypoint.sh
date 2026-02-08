#!/bin/bash
# Copyright (C) 2026 Ryan Daum <ryan.daum@gmail.com> This program is free
# software: you can redistribute it and/or modify it under the terms of the GNU
# General Public License as published by the Free Software Foundation, version
# 3.

set -e

# Configuration from environment
export RUN_DIR="${RUN_DIR:-run-cowbell}"
export DATA_DIR="/data"
export IMPORT_PATH="${IMPORT_PATH:-$DATA_DIR/cores/cowbell/src}"
export BUILD_PROFILE="${BUILD_PROFILE:-release-fast}"

# Paths within persistent volume
export IPC_DIR="$DATA_DIR/$RUN_DIR/ipc"
export CONFIG_DIR="$DATA_DIR/$RUN_DIR/config"
export MOOR_DATA_DIR="$DATA_DIR/$RUN_DIR/moor-data"
export EXPORT_DIR="$DATA_DIR/$RUN_DIR/export"
export TELNET_DATA_DIR="$DATA_DIR/$RUN_DIR/telnet-host-data"
export WEB_DATA_DIR="$DATA_DIR/$RUN_DIR/web-host-data"
export MCP_DATA_DIR="$DATA_DIR/$RUN_DIR/mcp-host-data"

# Runtime IPC socket directory
export RUNTIME_IPC="/var/run/moor"

echo "Starting mooR on Railway..."
echo "Data directory: $DATA_DIR"
echo "Run directory: $RUN_DIR"
echo "Build profile: $BUILD_PROFILE"

# Ensure directories exist
mkdir -p "$IPC_DIR" "$CONFIG_DIR" "$MOOR_DATA_DIR" "$EXPORT_DIR"
mkdir -p "$TELNET_DATA_DIR" "$WEB_DATA_DIR" "$MCP_DATA_DIR" "$RUNTIME_IPC"
rm -f "$RUNTIME_IPC"/*.sock 2>/dev/null || true

# Ensure cowbell core is available
if [ ! -d "$IMPORT_PATH" ]; then
    echo "Cowbell core not found at $IMPORT_PATH, fetching..."
    mkdir -p "$DATA_DIR/cores"
    if [ -f "/usr/local/bin/fetch-cowbell.sh" ]; then
        /usr/local/bin/fetch-cowbell.sh
    else
        echo "Warning: fetch-cowbell.sh not found, core may be missing"
    fi
fi

# Core features
export USE_BOOLEAN_RETURNS="${USE_BOOLEAN_RETURNS:-true}"
export CUSTOM_ERRORS="${CUSTOM_ERRORS:-true}"
export USE_UUOBJIDS="${USE_UUOBJIDS:-true}"
export ANONYMOUS_OBJECTS="${ANONYMOUS_OBJECTS:-true}"
export ENABLE_EVENTLOG="${ENABLE_EVENTLOG:-true}"
export MCP_CREATION_POLICY="${MCP_CREATION_POLICY:-open}"

# Start services in background

# 1. Start the daemon
echo "Starting moor-daemon..."
/usr/local/bin/moor-daemon \
    "$MOOR_DATA_DIR" \
    --db=development.db \
    --rpc-listen=ipc://$RUNTIME_IPC/rpc.sock,tcp://0.0.0.0:7899 \
    --events-listen=ipc://$RUNTIME_IPC/events.sock,tcp://0.0.0.0:7898 \
    --workers-response-listen=ipc://$RUNTIME_IPC/workers-response.sock \
    --workers-request-listen=ipc://$RUNTIME_IPC/workers-request.sock \
    --generate-keypair \
    --public-key=$CONFIG_DIR/moor.key.pub \
    --private-key=$CONFIG_DIR/moor.key.pem \
    --import="$IMPORT_PATH" \
    --import-format=objdef \
    --use-boolean-returns="$USE_BOOLEAN_RETURNS" \
    --custom-errors="$CUSTOM_ERRORS" \
    --use-uuobjids="$USE_UUOBJIDS" \
    --anonymous-objects="$ANONYMOUS_OBJECTS" \
    --enable-eventlog="$ENABLE_EVENTLOG" \
    --export="$EXPORT_DIR" \
    &

DAEMON_PID=$!

# Wait for daemon to be ready
echo "Waiting for daemon to start..."
for i in {1..30}; do
    if [ -S "$RUNTIME_IPC/rpc.sock" ]; then
        echo "Daemon is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "Timeout waiting for daemon"
        exit 1
    fi
    sleep 1
done

# 2. Start web host
echo "Starting moor-web-host..."
/usr/local/bin/moor-web-host \
    --listen-address=0.0.0.0:8081 \
    --rpc-address=ipc://$RUNTIME_IPC/rpc.sock \
    --events-address=ipc://$RUNTIME_IPC/events.sock \
    --data-dir="$WEB_DATA_DIR" \
    &

WEB_HOST_PID=$!

# 3. Start telnet host (optional, on port 8888)
echo "Starting moor-telnet-host..."
/usr/local/bin/moor-telnet-host \
    --telnet-address=0.0.0.0 \
    --telnet-port=8888 \
    --rpc-address=ipc://$RUNTIME_IPC/rpc.sock \
    --events-address=ipc://$RUNTIME_IPC/events.sock \
    --data-dir="$TELNET_DATA_DIR" \
    &

TELNET_PID=$!

# 4. Start curl worker
echo "Starting moor-curl-worker..."
/usr/local/bin/moor-curl-worker \
    --rpc-address=ipc://$RUNTIME_IPC/rpc.sock \
    --events-address=ipc://$RUNTIME_IPC/events.sock \
    --workers-request-address=ipc://$RUNTIME_IPC/workers-request.sock \
    --workers-response-address=ipc://$RUNTIME_IPC/workers-response.sock \
    &

CURL_PID=$!

# 5. Start nginx frontend
echo "Starting nginx..."
nginx -g "daemon off;" &
NGINX_PID=$!

# Health check function
check_health() {
    # Check if all processes are running
    kill -0 $DAEMON_PID 2>/dev/null || return 1
    kill -0 $WEB_HOST_PID 2>/dev/null || return 1
    kill -0 $TELNET_PID 2>/dev/null || return 1
    kill -0 $CURL_PID 2>/dev/null || return 1
    kill -0 $NGINX_PID 2>/dev/null || return 1
    return 0
}

# Graceful shutdown
shutdown() {
    echo "Shutting down..."
    kill -TERM $NGINX_PID 2>/dev/null || true
    kill -TERM $CURL_PID 2>/dev/null || true
    kill -TERM $TELNET_PID 2>/dev/null || true
    kill -TERM $WEB_HOST_PID 2>/dev/null || true
    kill -TERM $DAEMON_PID 2>/dev/null || true
    wait
    echo "Shutdown complete"
    exit 0
}

trap shutdown SIGTERM SIGINT

echo "All services started!"
echo "Ports: 8080 (frontend/web), 8081 (web API), 8888 (telnet)"
echo "Health check available at http://localhost:8080/health"

# Main loop - monitor services
while true; do
    if ! check_health; then
        echo "A service has died, shutting down..."
        shutdown
    fi
    sleep 5
done
