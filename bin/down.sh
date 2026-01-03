#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

PROFILE="${1:-}"

if [ -n "$PROFILE" ]; then
    echo "Stopping services with profile: $PROFILE"
    docker compose --profile "$PROFILE" rm -f --stop
else
    echo "Stopping services..."
    docker compose rm -f --stop
fi

echo "Services stopped successfully"
