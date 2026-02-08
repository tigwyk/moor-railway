#!/bin/bash
set -e

DATA_DIR="${DATA_DIR:-/data}"
RUN_DIR="${RUN_DIR:-run-cowbell}"
IMPORT_PATH="${IMPORT_PATH:-$DATA_DIR/cores/cowbell/src}"
CONFIG_DIR="$DATA_DIR/config/moor"
MOOR_DATA_DIR="$DATA_DIR/$RUN_DIR/moor-data"
EXPORT_DIR="$DATA_DIR/$RUN_DIR/export"

mkdir -p "$CONFIG_DIR" "$MOOR_DATA_DIR" "$EXPORT_DIR"

if [ ! -d "$IMPORT_PATH" ]; then
    echo "Fetching cowbell core..."
    /usr/local/bin/fetch-cowbell.sh
fi

if [ -n "$MOOR_ENROLLMENT_TOKEN" ]; then
    mkdir -p "$CONFIG_DIR"
    echo -n "$MOOR_ENROLLMENT_TOKEN" > "$CONFIG_DIR/enrollment-token"
    chmod 600 "$CONFIG_DIR/enrollment-token"
fi

exec /moor/moor-daemon \
    "$MOOR_DATA_DIR" \
    --db=development.db \
    --rpc-listen=tcp://0.0.0.0:7899 \
    --events-listen=tcp://0.0.0.0:7898 \
    --workers-response-listen=tcp://0.0.0.0:7897 \
    --workers-request-listen=tcp://0.0.0.0:7896 \
    --enrollment-listen=tcp://0.0.0.0:7900 \
    --generate-keypair \
    --public-key="$CONFIG_DIR/moor.key.pub" \
    --private-key="$CONFIG_DIR/moor.key.pem" \
    --import="$IMPORT_PATH" \
    --import-format=objdef \
    --use-boolean-returns="${USE_BOOLEAN_RETURNS:-true}" \
    --custom-errors="${CUSTOM_ERRORS:-true}" \
    --use-uuobjids="${USE_UUOBJIDS:-true}" \
    --anonymous-objects="${ANONYMOUS_OBJECTS:-true}" \
    --enable-eventlog="${ENABLE_EVENTLOG:-true}" \
    --export="$EXPORT_DIR"
