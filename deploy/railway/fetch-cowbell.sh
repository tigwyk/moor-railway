#!/bin/bash
# Script to fetch the cowbell core for Railway deployment.
# This version is adapted for the persistent volume layout.

set -e

COWBELL_DIR="/data/cores/cowbell"
COWBELL_REPO="timbran/cowbell"
CODEBERG_HOST="codeberg.org"

# Use HTTPS for Railway (no SSH keys available)
REPO_URL="https://${CODEBERG_HOST}/${COWBELL_REPO}.git"

if [ ! -d "$COWBELL_DIR" ]; then
    echo "Cloning cowbell from $REPO_URL..."
    git clone "$REPO_URL" "$COWBELL_DIR"
else
    echo "Cowbell directory already exists at $COWBELL_DIR"
    if [ -d "$COWBELL_DIR/.git" ]; then
        echo "Attempting to update cowbell..."
        (cd "$COWBELL_DIR" && git pull)
    else
        echo "Warning: $COWBELL_DIR exists but is not a git repository."
    fi
fi
