# Testing Ethos

This document describes how and why we test in Protectorate, and provides guidelines for maintaining tests as the codebase evolves.

## Why We Test

We test to enable confident refactoring. Tests are not about proving code works - they're about catching regressions when code changes. Our testing philosophy:

1. **Tests enable change** - Code without tests becomes frozen. Tests let us refactor aggressively.
2. **Tests document behavior** - A well-written test shows exactly what a function does.
3. **Tests catch regressions** - The primary value is knowing when something breaks.

We do NOT test for the sake of coverage metrics. We test code that:
- Has complex logic that's easy to get wrong
- Is called from multiple places (shared utilities)
- Has tricky edge cases
- Has broken before

## What We Test

### High-Value Targets

These should always have tests:

| Category | Examples | Why |
|----------|----------|-----|
| **Parsing/Formatting** | URL parsing, SSE encoding, duration parsing | Easy to break, hard to debug in production |
| **Caching Logic** | TTL expiration, concurrent access | Race conditions are subtle |
| **Configuration** | Env var parsing, defaults, overrides | Misconfiguration causes silent failures |
| **Pure Functions** | Transformations with no side effects | Easy to test, high value |

### Low-Value Targets (Skip These)

Don't write tests for:
- Simple getters/setters
- Code that just delegates to well-tested libraries
- One-line wrapper functions
- Code that requires complex mocking to test

### The Interface Boundary

We use interfaces to define testable boundaries. The pattern:

```
+------------------+     +-------------------+
|   Real Code      |     |   Test Code       |
|                  |     |                   |
| DockerClient     |     | MockDockerClient  |
| (concrete)       |     | (implements same  |
|                  |     |  interface)       |
+--------+---------+     +---------+---------+
         |                         |
         v                         v
    +----+-------------------------+----+
    |        DockerExecClient           |
    |        (interface)                |
    +-----------------------------------+
```

Interfaces live in `internal/envoy/interfaces.go`. Each interface is focused:
- `DockerExecClient` - terminal attachment
- `DockerContainerStatsClient` - resource monitoring
- `DockerContainerLifecycleClient` - spawn/kill operations

## How We Test

### Running Tests

```bash
make test          # Run with race detector (default)
make test-unit     # Fast, no race detector
make test-race     # With race detector
make test-cover    # With coverage report
make ci            # Lint + tests (what CI runs)
```

### Test Patterns

**Table-driven tests** for functions with multiple cases:

```go
func TestParseDuration(t *testing.T) {
    tests := []struct {
        name       string
        input      string
        defaultVal time.Duration
        want       time.Duration
    }{
        {"parses hours", "2h", time.Hour, 2 * time.Hour},
        {"returns default on empty", "", time.Hour, time.Hour},
        {"returns default on invalid", "invalid", time.Hour, time.Hour},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            got := parseDuration(tt.input, tt.defaultVal)
            if got != tt.want {
                t.Errorf("parseDuration() = %v, want %v", got, tt.want)
            }
        })
    }
}
```

**httptest for HTTP handlers**:

```go
func TestSidecarClient_GetStatus_Success(t *testing.T) {
    server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        json.NewEncoder(w).Encode(&SidecarStatus{SleeveName: "test"})
    }))
    defer server.Close()

    // Test against the mock server
}
```

**Synchronization for concurrent tests**:

```go
func waitForCount(hub *SSEHub, expected int, maxAttempts int) {
    for i := 0; i < maxAttempts; i++ {
        if hub.ClientCount() == expected {
            return
        }
        time.Sleep(time.Millisecond)
    }
}
```

### File Organization

```
internal/
  config/
    config.go           # Implementation
    config_test.go      # Tests for config.go
  envoy/
    interfaces.go       # Shared interfaces
    sleeve_manager.go   # Implementation
    sleeve_manager_test.go  # Tests (if needed)
    sse.go              # Implementation
    sse_test.go         # Tests for SSE
```

## Guidelines for New Features

When adding a new feature:

1. **Ask: Is this testable?**
   - If it requires mocking 5 dependencies, reconsider the design
   - Extract pure logic into separate functions that are easy to test

2. **Ask: What could break?**
   - Test the edge cases, not the happy path
   - Empty inputs, nil values, boundary conditions

3. **Add interface methods if needed**
   - New Docker operations go in `interfaces.go`
   - Implement the interface method in `docker.go`

4. **Write the test first if the logic is complex**
   - Helps clarify what the function should actually do
   - Not dogmatic TDD - just when it helps

### Example: Adding a New Feature

Say we're adding workspace deletion:

```go
// 1. Add to interface (interfaces.go)
type WorkspaceDeleter interface {
    Delete(path string) error
}

// 2. Implement (workspace_manager.go)
func (wm *WorkspaceManager) Delete(path string) error {
    // Check not in use, then delete
}

// 3. Test the edge cases (workspace_manager_test.go)
func TestWorkspaceManager_Delete(t *testing.T) {
    tests := []struct {
        name    string
        path    string
        inUse   bool
        wantErr bool
    }{
        {"deletes unused workspace", "/ws/test", false, false},
        {"rejects workspace in use", "/ws/active", true, true},
        {"rejects non-existent", "/ws/missing", false, true},
    }
    // ...
}
```

## Guidelines for Feature Changes

When modifying existing behavior:

1. **Run existing tests first**
   ```bash
   make test
   ```
   If they pass, you have a baseline.

2. **Update tests to reflect new behavior**
   - If a function's contract changes, update the test expectations
   - Don't delete tests to make them pass - understand why they fail

3. **Add tests for new edge cases**
   - If you're fixing a bug, add a test that would have caught it

4. **Remove tests only when behavior is intentionally removed**
   - Document why in the commit message

### Example: Changing URL Parsing

If `repoNameFromURL` behavior changes:

```go
// Before: "https://github.com/user/repo.git/" returned "repo.git"
// After: "https://github.com/user/repo.git/" returns "repo"

func TestRepoNameFromURL(t *testing.T) {
    tests := []struct {
        url  string
        want string
    }{
        // Update expectation to match new behavior
        {"https://github.com/user/repo.git/", "repo"},  // Changed
        // Keep other cases
        {"https://github.com/user/repo", "repo"},
    }
}
```

## CI Integration

Every push runs:
- `go test -race ./...` - catches race conditions
- `golangci-lint` - catches common mistakes

Every release additionally requires tests to pass before building Docker images.

### When CI Fails

1. **Test failure**: Something broke. Fix it before merging.
2. **Race detected**: You have concurrent access issues. This is serious.
3. **Lint failure**: Usually a quick fix (unused variable, etc.)

## What We Don't Do

- **100% coverage goals** - Coverage is a tool, not a target
- **Mocking everything** - If you need 10 mocks, redesign
- **Testing private functions** - Test through the public API
- **Brittle UI tests** - Our tests focus on logic, not HTML output
- **Slow integration tests** - Keep the test suite fast (<10s)

## Summary

| Do | Don't |
|----|-------|
| Test parsing, caching, config | Test simple delegation |
| Use table-driven tests | Write one test per file |
| Test edge cases | Test only happy paths |
| Keep tests fast | Add slow integration tests |
| Update tests with features | Delete tests to make CI pass |
| Use interfaces for boundaries | Mock everything |
