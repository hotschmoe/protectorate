.PHONY: build build-base build-sleeve build-envoy build-envoy-release up down clean clean-all help test test-unit test-race test-cover lint ci

# Default target (dev build)
build: build-envoy build-sleeve

# Build the shared base image (slow, run once)
build-base:
	DOCKER_BUILDKIT=1 docker build \
		--provenance=false \
		-t protectorate/base:latest \
		-f containers/base/Dockerfile \
		containers/base/

# Build the sleeve image (fast, uses base)
build-sleeve: bin/sidecar
	DOCKER_BUILDKIT=1 docker build \
		--provenance=false \
		-t protectorate/sleeve:latest \
		-f containers/sleeve/Dockerfile \
		.

# Build envoy for dev (fast: local Go build + copy binary)
build-envoy: bin/envoy
	DOCKER_BUILDKIT=1 docker build \
		--provenance=false \
		-t protectorate/envoy:latest \
		-f containers/envoy/Dockerfile.dev \
		.

# Build Go binaries locally
bin/envoy: $(shell find . -name '*.go' -type f)
	@mkdir -p bin
	CGO_ENABLED=0 GOOS=linux go build -o bin/envoy ./cmd/envoy

bin/sidecar: $(shell find . -name '*.go' -type f)
	@mkdir -p bin
	CGO_ENABLED=0 GOOS=linux go build -o bin/sidecar ./cmd/sidecar

# Build envoy for release (slow: multi-stage, self-contained)
build-envoy-release:
	DOCKER_BUILDKIT=1 docker build \
		--provenance=false \
		-t protectorate/envoy:latest \
		-f containers/envoy/Dockerfile \
		.

# Build everything including base (for CI or fresh setup)
build-all: build-base build-envoy-release build-sleeve

# Run docker-compose
up:
	docker compose up -d

down:
	docker compose down

# Remove all protectorate containers and networks
clean:
	@echo "Stopping and removing all sleeve containers..."
	@docker ps -aq --filter "name=sleeve-" | xargs -r docker rm -f 2>/dev/null || true
	@echo "Stopping envoy..."
	@docker compose down 2>/dev/null || true
	@echo "Removing raven network..."
	@docker network rm raven 2>/dev/null || true
	@echo "Clean complete."

# Nuclear option: clean + remove all protectorate images
clean-all: clean
	@echo "Removing protectorate images..."
	@docker rmi protectorate/sleeve:latest 2>/dev/null || true
	@docker rmi protectorate/base:latest 2>/dev/null || true
	@docker rmi protectorate/envoy:latest 2>/dev/null || true
	@echo "Clean-all complete."

# =============================================================================
# Testing Targets
# =============================================================================

test: test-race

test-unit:
	go test ./...

test-race:
	go test -race ./...

test-cover:
	go test -race -coverprofile=coverage.out ./...
	go tool cover -func=coverage.out
	@rm -f coverage.out

lint:
	@if command -v golangci-lint >/dev/null 2>&1; then \
		golangci-lint run ./...; \
	else \
		echo "golangci-lint not installed, skipping lint"; \
	fi

ci: lint test-race

# Show build times
time-base:
	time $(MAKE) build-base

time-sleeve:
	time $(MAKE) build-sleeve

time-envoy:
	time $(MAKE) build-envoy

# =============================================================================
# Development Targets (fast iteration with volume mounts)
# =============================================================================

.PHONY: dev dev-down dev-logs dev-restart watch

# Start development environment with volume-mounted binary and hot-reload webui
dev: bin/envoy
	@echo "Starting dev environment..."
	@echo "  - Binary mounted from ./bin/envoy"
	@echo "  - Webui hot-reload enabled (just refresh browser)"
	@echo "  - API available at http://localhost:7470"
	@echo ""
	docker compose -f docker-compose.dev.yaml up -d
	@echo ""
	@echo "Dev environment ready!"
	@echo "  Webui: http://localhost:7470"
	@echo "  Logs:  make dev-logs"
	@echo ""
	@echo "Workflow:"
	@echo "  Webui changes: Edit HTML -> Refresh browser (instant)"
	@echo "  Go changes:    make dev-restart (~5 sec)"

# Stop development environment
dev-down:
	docker compose -f docker-compose.dev.yaml down

# View development logs
dev-logs:
	docker compose -f docker-compose.dev.yaml logs -f

# Restart envoy with latest Go code and compose changes
dev-restart: bin/envoy
	docker compose -f docker-compose.dev.yaml up -d --force-recreate envoy
	@echo "Envoy restarted"

# Auto-rebuild on Go file changes (Phase 2 - requires inotify-tools)
watch:
	@chmod +x ./scripts/watch.sh
	@./scripts/watch.sh

# =============================================================================
# Production/Release Targets
# =============================================================================

.PHONY: release publish install

GHCR_REGISTRY := ghcr.io/hotschmoe
VERSION ?= latest

# Build production containers (full multi-stage builds)
release: build-all
	@echo "Production build complete"

# Build and push images to ghcr.io (fallback if GitHub Actions fails)
# Usage: make publish VERSION=v0.1.0
# Requires: docker login ghcr.io -u USERNAME -p TOKEN
publish: build-all
	@echo "Tagging and pushing images to $(GHCR_REGISTRY)..."
	docker tag protectorate/envoy:latest $(GHCR_REGISTRY)/protectorate-envoy:$(VERSION)
	docker tag protectorate/envoy:latest $(GHCR_REGISTRY)/protectorate-envoy:latest
	docker tag protectorate/sleeve:latest $(GHCR_REGISTRY)/protectorate-sleeve:$(VERSION)
	docker tag protectorate/sleeve:latest $(GHCR_REGISTRY)/protectorate-sleeve:latest
	docker push $(GHCR_REGISTRY)/protectorate-envoy:$(VERSION)
	docker push $(GHCR_REGISTRY)/protectorate-envoy:latest
	docker push $(GHCR_REGISTRY)/protectorate-sleeve:$(VERSION)
	docker push $(GHCR_REGISTRY)/protectorate-sleeve:latest
	@echo "Published $(VERSION) to $(GHCR_REGISTRY)"

# Run the install script (for local testing)
install:
	@chmod +x install.sh
	@./install.sh

# =============================================================================
# Help
# =============================================================================

help:
	@echo "Protectorate Build Targets"
	@echo ""
	@echo "Development (fast iteration):"
	@echo "  make dev                 Start dev environment (volume-mounted)"
	@echo "  make dev-restart         Rebuild Go + recreate container (picks up all changes)"
	@echo "  make dev-logs            View dev container logs"
	@echo "  make dev-down            Stop dev environment"
	@echo "  make watch               Auto-rebuild on file changes (requires inotify-tools)"
	@echo ""
	@echo "Testing:"
	@echo "  make test                Run tests with race detection"
	@echo "  make test-unit           Run unit tests (fast, no race)"
	@echo "  make test-race           Run tests with race detector"
	@echo "  make test-cover          Run tests with coverage report"
	@echo "  make lint                Run golangci-lint (if installed)"
	@echo "  make ci                  Run lint + tests (for CI)"
	@echo ""
	@echo "Container Builds:"
	@echo "  make build-base          Build shared base image (slow, ~2 min, run once)"
	@echo "  make build-sleeve        Build sleeve image (fast, ~3 sec)"
	@echo "  make build-envoy         Build envoy for dev (fast, ~3 sec, local Go)"
	@echo "  make build-envoy-release Build envoy for release (slow, multi-stage)"
	@echo "  make build               Build envoy + sleeve for dev"
	@echo "  make build-all           Build everything for release (includes base)"
	@echo ""
	@echo "Production:"
	@echo "  make release             Full production build"
	@echo "  make publish VERSION=v0.1.0  Build and push to ghcr.io"
	@echo "  make up                  Start services via docker-compose"
	@echo "  make down                Stop services"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean               Remove all containers and networks"
	@echo "  make clean-all           Remove containers, networks, and images"
