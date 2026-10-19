#!/usr/bin/env bash
#
# Protectorate Uninstaller
# https://github.com/hotschmoe/protectorate
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/uninstall.sh | bash
#
# This script:
#   1. Stops all sleeve containers
#   2. Stops envoy container
#   3. Removes protectorate containers
#   4. Removes raven network
#   5. Optionally removes container images
#   6. Optionally removes ~/protectorate directory
#
set -e

PROTECTORATE_DIR="$HOME/protectorate"

# Colors
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

confirm() {
    local prompt="$1"
    local default="${2:-n}"

    if [[ "$default" == "y" ]]; then
        prompt="$prompt [Y/n] "
    else
        prompt="$prompt [y/N] "
    fi

    read -p "$prompt" response
    response="${response:-$default}"

    [[ "$response" =~ ^[Yy]$ ]]
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

main() {
    echo ""
    echo "========================================"
    echo "  Protectorate Uninstaller"
    echo "========================================"
    echo ""

    # Check if docker is available
    if ! command -v docker &> /dev/null; then
        error "Docker not found. Nothing to uninstall."
        exit 1
    fi

    # Stop and remove sleeve containers
    info "Finding sleeve containers..."
    SLEEVE_CONTAINERS=$(docker ps -aq --filter "name=sleeve-" 2>/dev/null || true)
    if [[ -n "$SLEEVE_CONTAINERS" ]]; then
        info "Stopping and removing sleeve containers..."
        echo "$SLEEVE_CONTAINERS" | xargs -r docker rm -f 2>/dev/null || true
        success "Sleeve containers removed"
    else
        info "No sleeve containers found"
    fi

    # Stop and remove envoy container
    info "Checking for envoy container..."
    if docker ps -aq --filter "name=envoy-poe" | grep -q .; then
        info "Stopping and removing envoy container..."
        docker rm -f envoy-poe 2>/dev/null || true
        success "Envoy container removed"
    elif docker ps -aq --filter "name=envoy-dev" | grep -q .; then
        info "Stopping and removing envoy-dev container..."
        docker rm -f envoy-dev 2>/dev/null || true
        success "Envoy-dev container removed"
    else
        info "No envoy container found"
    fi

    # Remove raven network
    info "Checking for raven network..."
    if docker network ls | grep -q "raven"; then
        info "Removing raven network..."
        docker network rm raven 2>/dev/null || true
        success "Raven network removed"
    else
        info "Raven network not found"
    fi

    echo ""

    # Ask about removing images
    if confirm "Remove Protectorate container images?"; then
        info "Removing images..."
        docker rmi ghcr.io/hotschmoe/protectorate-envoy:latest 2>/dev/null || true
        docker rmi ghcr.io/hotschmoe/protectorate-sleeve:latest 2>/dev/null || true
        docker rmi protectorate/envoy:latest 2>/dev/null || true
        docker rmi protectorate/sleeve:latest 2>/dev/null || true
        docker rmi protectorate/base:latest 2>/dev/null || true
        success "Images removed"
    else
        info "Keeping images"
    fi

    # Ask about removing protectorate directory
    if [[ -d "$PROTECTORATE_DIR" ]]; then
        echo ""
        warn "Found $PROTECTORATE_DIR"
        if confirm "Remove Protectorate directory? (This deletes all workspaces!)"; then
            info "Removing $PROTECTORATE_DIR..."
            rm -rf "$PROTECTORATE_DIR"
            success "Directory removed"
        else
            info "Keeping directory"
        fi
    fi

    echo ""
    echo "========================================"
    echo "  Uninstall complete"
    echo "========================================"
    echo ""
    echo "The following were NOT removed:"
    echo "  - Docker (may be used by other apps)"
    echo "  - Claude CLI (may be used standalone)"
    echo "  - ~/.claude/ credentials"
    echo ""
    echo "To remove Claude CLI: rm -rf ~/.local/bin/claude ~/.claude"
    echo ""
}

main "$@"
