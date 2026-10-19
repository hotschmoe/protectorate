#!/bin/bash
# scripts/watch.sh
# Phase 2: Auto-rebuild on Go file changes
#
# Prerequisites:
#   sudo apt-get install inotify-tools
#
# Usage:
#   make watch
#   # or directly: ./scripts/watch.sh

set -e

echo "=========================================="
echo "  Protectorate Dev Watcher"
echo "=========================================="
echo ""
echo "Watching for changes in:"
echo "  - cmd/**/*.go"
echo "  - internal/**/*.go"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

rebuild() {
    echo -e "${YELLOW}[$(date +%H:%M:%S)] Change detected, rebuilding...${NC}"

    if make bin/envoy 2>&1; then
        echo -e "${GREEN}[$(date +%H:%M:%S)] Build successful, restarting envoy...${NC}"
        if docker compose -f docker-compose.dev.yaml restart envoy 2>&1; then
            echo -e "${GREEN}[$(date +%H:%M:%S)] Ready! Refresh browser to see changes.${NC}"
        else
            echo -e "${RED}[$(date +%H:%M:%S)] Failed to restart container${NC}"
        fi
    else
        echo -e "${RED}[$(date +%H:%M:%S)] Build failed!${NC}"
    fi
    echo ""
}

# Check for inotifywait
if ! command -v inotifywait &> /dev/null; then
    echo -e "${RED}Error: inotifywait not found${NC}"
    echo ""
    echo "Install with:"
    echo "  sudo apt-get install inotify-tools"
    exit 1
fi

# Check if dev environment is running
if ! docker compose -f docker-compose.dev.yaml ps --quiet envoy 2>/dev/null | grep -q .; then
    echo -e "${YELLOW}Dev environment not running. Starting...${NC}"
    make dev
    echo ""
fi

# Initial build
rebuild

# Watch for changes
while true; do
    # Wait for any .go file to change
    inotifywait -r -e modify,create,delete \
        --include '.*\.go$' \
        ./cmd ./internal 2>/dev/null

    # Small delay to batch rapid changes (e.g., save-all in IDE)
    sleep 0.3

    rebuild
done
