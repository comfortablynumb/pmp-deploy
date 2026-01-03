#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

PROFILE="${1:-}"

# Run down first
"$SCRIPT_DIR/down.sh" "$PROFILE"

if [ -n "$PROFILE" ]; then
    echo "Starting services with profile: $PROFILE"
    docker compose --profile "$PROFILE" up -d
else
    echo "Starting services..."
    docker compose up -d
fi

echo "Services started successfully"
