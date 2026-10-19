#!/usr/bin/env bash
#
# Protectorate Installer
# https://github.com/hotschmoe/protectorate
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/install.sh | bash
#
set -e

PROTECTORATE_DIR="$HOME/protectorate"
RAW_GITHUB="https://raw.githubusercontent.com/hotschmoe/protectorate/master"

info() { echo -e "\033[0;34m[INFO]\033[0m $1"; }
success() { echo -e "\033[0;32m[OK]\033[0m $1"; }
error() { echo -e "\033[0;31m[ERROR]\033[0m $1" >&2; exit 1; }

# Check Docker
command -v docker &>/dev/null || error "Docker required. Install from https://docker.com"
docker info &>/dev/null || error "Docker not running. Please start Docker and try again."

# Setup directory
info "Setting up $PROTECTORATE_DIR..."
mkdir -p "$PROTECTORATE_DIR"
cd "$PROTECTORATE_DIR"

# Download docker-compose.yaml
info "Downloading docker-compose.yaml..."
curl -fsSL "$RAW_GITHUB/docker-compose.yaml" -o docker-compose.yaml

# Pull images
info "Pulling container images..."
docker pull ghcr.io/hotschmoe/protectorate-envoy:latest
docker pull ghcr.io/hotschmoe/protectorate-sleeve:latest

# Start services
info "Starting Envoy..."
docker compose down 2>/dev/null || true
docker compose up -d

# Wait for health
info "Waiting for Envoy to start..."
for i in {1..30}; do
    curl -s http://localhost:7470/health &>/dev/null && break
    sleep 1
done
curl -s http://localhost:7470/health &>/dev/null || error "Envoy did not start. Check: docker logs envoy"

success "Protectorate is running!"
echo ""
echo "WebUI:  http://localhost:7470"
echo "Logs:   docker logs -f envoy"
echo "Stop:   cd ~/protectorate && docker compose down"
echo ""
echo "Next steps:"
echo "  1. Open http://localhost:7470"
echo "  2. Use 'envoy auth login claude --token <TOKEN>' to authenticate"
echo "  3. Clone a repo and spawn a sleeve"
echo ""
