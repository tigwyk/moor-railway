#!/bin/bash
set -e

DATA_DIR="${DATA_DIR:-/data}"
DAEMON_HOST="${DAEMON_HOST:-daemon.railway.internal}"
DAEMON_ENROLLMENT_PORT="${DAEMON_ENROLLMENT_PORT:-7900}"
DAEMON_RPC_PORT="${DAEMON_RPC_PORT:-7899}"
DAEMON_EVENTS_PORT="${DAEMON_EVENTS_PORT:-7898}"
DAEMON_WORKERS_REQUEST_PORT="${DAEMON_WORKERS_REQUEST_PORT:-7896}"
DAEMON_WORKERS_RESPONSE_PORT="${DAEMON_WORKERS_RESPONSE_PORT:-7897}"
WORKER_DATA_DIR="$DATA_DIR/worker-data"

mkdir -p "$WORKER_DATA_DIR"

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

exec /moor/moor-curl-worker \
    --rpc-address="tcp://$DAEMON_HOST:$DAEMON_RPC_PORT" \
    --events-address="tcp://$DAEMON_HOST:$DAEMON_EVENTS_PORT" \
    --workers-request-address="tcp://$DAEMON_HOST:$DAEMON_WORKERS_REQUEST_PORT" \
    --workers-response-address="tcp://$DAEMON_HOST:$DAEMON_WORKERS_RESPONSE_PORT" \
    --enrollment-address="tcp://$DAEMON_HOST:$DAEMON_ENROLLMENT_PORT" \
    --data-dir="$WORKER_DATA_DIR"
