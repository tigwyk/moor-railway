#!/bin/bash
set -e

DATA_DIR="${DATA_DIR:-/data}"
DAEMON_HOST="${DAEMON_HOST:-daemon.railway.internal}"
DAEMON_ENROLLMENT_PORT="${DAEMON_ENROLLMENT_PORT:-7900}"
DAEMON_RPC_PORT="${DAEMON_RPC_PORT:-7899}"
DAEMON_EVENTS_PORT="${DAEMON_EVENTS_PORT:-7898}"
WEB_DATA_DIR="$DATA_DIR/web-host-data"

mkdir -p "$WEB_DATA_DIR"

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

exec /moor/moor-web-host \
    --listen-address=0.0.0.0:8081 \
    --rpc-address="tcp://$DAEMON_HOST:$DAEMON_RPC_PORT" \
    --events-address="tcp://$DAEMON_HOST:$DAEMON_EVENTS_PORT" \
    --enrollment-address="tcp://$DAEMON_HOST:$DAEMON_ENROLLMENT_PORT" \
    --data-dir="$WEB_DATA_DIR"
