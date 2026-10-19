---
name: test
description: Run Go tests with smart defaults
---

# /test - Run Go Tests

Run Go tests with race detection and coverage.

## Usage

```
/test [--race] [--cover] [--verbose] [package]
```

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--race` | enabled | Enable race detector |
| `--cover` | disabled | Generate coverage report |
| `--verbose` | disabled | Verbose test output |
| `package` | ./... | Package pattern to test |

## Examples

- `/test` - Run all tests with race detection
- `/test --cover` - Run with coverage report
- `/test ./internal/manager/...` - Test manager package only
- `/test --verbose` - Verbose output for debugging

## What It Does

1. Executes: `go test -race [options] [package]`
2. Reports PASS/FAIL with test count and duration
3. Shows coverage percentage if enabled
4. Exits 0 on success, 1 on failure

## When to Use

- After making code changes
- Before committing
- Quick regression check
- CI validation

## Implementation

```bash
# Default (race detection, all packages)
go test -race ./...

# With coverage
go test -race -coverprofile=coverage.out ./...
go tool cover -func=coverage.out

# Verbose
go test -race -v ./...
```
