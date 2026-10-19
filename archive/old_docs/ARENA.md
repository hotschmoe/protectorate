# Cortical Arena: Agent Benchmarking System

**Objective**: Provide a standardized environment to benchmark and compare the performance of different AI CLI agents (Claude Code, OpenCode, Aider, etc.) on identical tasks.

## Concept

The **Arena** allows the manager to spawn multiple "contestant" containers, each running a different agent configuration, but all starting from the exact same state (file system + stack). This enables direct A/B testing, cost analysis, and capability benchmarking.

## Use Cases

1.  **Model Comparison**: How does Claude 3.7 Sonnet compare to GPT-4o on the same refactor task?
2.  **Tool Comparison**: Is Aider faster than Claude Code for this specific bug fix?
3.  **Cost Analysis**: Agent A solved it in 5 mins ($0.50), Agent B in 2 mins ($1.20). Which is better?
4.  **Resilience Testing**: Start from a "crashed" state (mid-refactor) and see which agent recovers best.

## Workflow

### 1. Challenge Definition

A "Challenge" is a frozen state of a workspace, representing the starting line.

*   **Repo**: The git repository at a specific commit.
*   **Stack**: The `.stack/` directory containing `CURRENT.md`, `PLAN.md`, `INBOX.md` (goal).
*   **Scenario Types**:
    *   **Fresh Start**: Empty stack (except `INBOX.md` with goal). Simulates a new feature request.
    *   **Mid-Flight Recovery**: Stack populated with `CURRENT.md` and `PLAN.md` from a previous session (perhaps one that crashed). Simulates taking over ongoing work.

### 2. The Arena Configuration

Users define the match using a JSON/YAML configuration:

```json
{
  "challenge_id": "refactor-auth-module",
  "challenge_source": {
    "repo": "https://github.com/example/repo.git",
    "commit": "a1b2c3d4",
    "stack_snapshot": "./snapshots/auth-refactor-crash.tar.gz"
  },
  "contestants": [
    { "name": "claude-sonnet", "cli": "claude-code", "model": "sonnet-3.7" },
    { "name": "claude-opus", "cli": "claude-code", "model": "opus-3" },
    { "name": "aider-gpt4", "cli": "aider", "model": "gpt-4o" }
  ],
  "success_criteria": {
    "test_command": "go test ./auth/...",
    "timeout": "30m"
  }
}
```

### 3. Execution (The Race)

The Manager orchestrates the arena:

1.  **Preparation**: Creates N isolated workspaces (e.g., `/workspaces/arena-run-1/contestant-A`, `/workspaces/arena-run-1/contestant-B`).
2.  **Hydration**:
    *   Clones the repo to each workspace.
    *   Checks out the specific commit.
    *   Extracts the stack snapshot (or creates the fresh stack) into `.stack/`.
3.  **Spawn**: Spawns a container for each contestant, mounting their respective workspace.
4.  **Start**: Starts the clock.

### 4. Metrics & Monitoring

The Sidecar and Manager track performance metrics:

*   **Time to Success**: Wall-clock time until `success_criteria` passes.
*   **Cost**: API costs.
    *   *Implementation Note*: Since not all CLIs report cost, we may need a proxy or rely on the CLI's own reporting if available (e.g., parsing `claude-code` output).
*   **Steps/Interactions**: Number of interactions (LLM turns) or commands executed.
*   **Resource Usage**: CPU/Memory usage of the container.

### 5. Verification & Judgment

How do we know who won?

*   **Passive Verification**: Agent marks task as done in `OUTBOX.md`. Manager triggers the `success_criteria` (e.g., runs the test suite).
*   **Timeout**: If an agent hits the timeout, it is marked as DNF (Did Not Finish).
*   **Quality Check**: (Optional) Run a linter or complexity analysis on the resulting code.

### 6. Reporting

The system generates a report:

```markdown
# Arena Report: Refactor Auth Module

| Contestant | Status | Time | Cost (Est) | Tests Passed |
|------------|--------|------|------------|--------------|
| Claude Sonnet | SUCCESS | 12m 30s | $0.45 | 100% |
| Aider GPT-4 | SUCCESS | 8m 15s | $1.10 | 100% |
| Claude Opus | TIMEOUT | > 30m | $2.50 | 40% |
```

## Implementation Details

### Stack Management

To support "Mid-Flight Recovery", the system needs a way to snapshot a stack.

*   `cortical snapshot <agent-id> <output-file>`: Zips up the `.stack/` directory.
*   This snapshot is then used as the seed for the Arena.

### Cost Tracking Challenges

Accurate cost tracking is difficult without a proxy.
*   **Phase 1**: Rely on time and rough token estimates (if logs are accessible).
*   **Phase 2**: Implement an OpenAI-compatible proxy within the Manager that agents are configured to use. This allows exact token counting and cost calculation for any tool using standard APIs.

### User Interface

*   **CLI**: `cortical arena run <config.json>`
*   **Dashboard**: A new "Arena" tab showing active races with progress bars and live logs from each contestant.
