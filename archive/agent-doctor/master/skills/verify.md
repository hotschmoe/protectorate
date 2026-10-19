---
name: verify
description: Build all Protectorate components and verify consistency
agent: build-verifier
---

# /verify - Build Verification

Build all Go binaries and Docker images, run tests.

## Usage

```
/verify [--quick] [--docker]
```

## Options

| Option | Description |
|--------|-------------|
| `--quick` | Only build Go binaries, skip Docker and tests |
| `--docker` | Include Docker image builds |

## What It Does

1. Builds Go binaries (cmd/envoy, cmd/sidecar)
2. Runs Go tests with race detection
3. Builds Docker images (if --docker)
4. Reports build times and any failures

## Output Format

```
BUILD VERIFICATION REPORT
-------------------------
cmd/envoy:    PASS (2.3s)
cmd/sidecar:  PASS (1.8s)
go test:      PASS (15.2s, 47 tests)

DOCKER IMAGES (if --docker):
protectorate-envoy:   PASS
protectorate-sleeve:  PASS

RESULT: ALL COMPONENTS PASS
```

## When to Use

- Before merging PRs
- After significant refactors
- Before releases
- When changing build system

## Implementation

```bash
# Build Go binaries
go build -o bin/envoy ./cmd/envoy
go build -o bin/sidecar ./cmd/sidecar

# Run tests
go test -race ./...

# Build Docker images (if --docker)
docker build -f containers/envoy/Dockerfile -t protectorate-envoy .
docker build -f containers/sleeve/Dockerfile -t protectorate-sleeve .
```

## Exit Codes

- 0: All components build and pass
- 1: Build failure or test failure
