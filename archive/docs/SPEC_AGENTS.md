# SPEC_AGENTS.md

Specification for Claude Code configuration management across Protectorate sleeves and workspaces.

**Status**: Draft - Gathering Requirements
**Last Updated**: 2026-01-27

---

## Problem Statement

Two distinct problems:

1. **Sleeve Isolation**: When running multiple Claude Code sleeves, each container has isolated `~/.claude/` configuration. Users want consistent tooling (plugins, settings) across all sleeves without manual per-sleeve setup.

2. **Workspace Consistency**: When managing multiple git repositories/workspaces, project-agnostic configuration (coding philosophies, common commands, tool usage patterns) must be manually duplicated. Users want a single source of truth for shared configuration.

---

## Configuration Ownership Model

```
+------------------------------------------------------------------+
|                    CONFIGURATION SOURCES                          |
+------------------------------------------------------------------+
|                                                                   |
|  GIT-CONTROLLED (per repo)          USER-CONTROLLED (per machine) |
|  -------------------------          ----------------------------- |
|  Travels with the code              Stays on the machine          |
|                                                                   |
|  <repo>/.claude/                    ~/.claude/                    |
|    agents/                            plugins/                    |
|    skills/                            settings.json               |
|    settings.json                      commands/                   |
|    settings.local.json (gitignored)   .credentials.json           |
|  <repo>/.mcp.json                   ~/.claude.json (MCP)          |
|  <repo>/CLAUDE.md                                                 |
|                                                                   |
+------------------------------------------------------------------+
```

**Key Insight**: `.claude/` and `CLAUDE.md` are NOT machine-dependent. They live in the git repo and travel with the code. The "inheritance" problem only applies to user-level config (`~/.claude/`).

---

## Current State

### What Sleeves Get (via workspace mount)

| Config Type | Source | Notes |
|-------------|--------|-------|
| CLAUDE.md | Git repo | Project instructions |
| .claude/agents/ | Git repo | Project agents |
| .claude/skills/ | Git repo | Project commands |
| .claude/settings.json | Git repo | Project settings |
| .claude/settings.local.json | Git repo (gitignored) | Local overrides |
| .mcp.json | Git repo | Project MCP servers |

### What Sleeves DON'T Get (user-level, isolated per container)

| Config Type | Location | Notes |
|-------------|----------|-------|
| Plugins | ~/.claude/plugins/ | Container has own ~/.claude/ |
| User settings | ~/.claude/settings.json | Plugin enablement |
| User commands | ~/.claude/commands/ | If any |
| User MCP | ~/.claude.json | Global MCP servers |
| Hooks | Inside plugins | Bundled with plugins |
| Credentials | ~/.claude/.credentials.json | NEVER share |

---

## Proposed Solutions

### Solution A: Agent Doctor (Workspace Sync)

A tool that maintains consistency of project-agnostic configuration across all workspaces.

```
+---------------------------+
|     MASTER REFERENCE      |
|  ~/.protectorate/master/  |
|    CLAUDE.md.common       |
|    agents/                |
|    skills/                |
|    hooks/                 |
|    commands/              |
+------------+--------------+
             |
             | agent-doctor sync
             |
    +--------+--------+--------+
    |        |        |        |
    v        v        v        v
  repo-a   repo-b   repo-c   repo-d
  .claude/ .claude/ .claude/ .claude/
  CLAUDE.md ...      ...      ...
```

**What Agent Doctor Manages** (project-agnostic content):
- Common CLAUDE.md sections (coding philosophies, tool usage, general rules)
- Shared agents (build-verifier, code-simplifier wrappers, etc.)
- Shared skills (common commands like /test, /build, /docker)
- Shared hooks (formatting, linting patterns)
- Coding standards and conventions

**What Agent Doctor Does NOT Manage** (project-specific):
- Project architecture documentation
- Project-specific agents (domain-specific)
- Project-specific skills
- API documentation
- Build commands unique to the project

**Master Reference Location** (DECIDED: in protectorate repo):
```
protectorate/
└── agent-doctor/
    ├── master/
    │   ├── CLAUDE.md.common      # Common sections to inject
    │   ├── agents/               # Shared agent definitions
    │   ├── skills/               # Shared skill definitions
    │   └── hooks/                # Shared hook patterns
    └── config.yaml               # Which workspaces to manage
```

**Sync Strategy** (DECIDED: injection for CLAUDE.md, copy for files):

For CLAUDE.md - **Injection with markers**:
```markdown
# CLAUDE.md

<!-- BEGIN PROTECTORATE COMMON -->
[auto-managed content - DO NOT EDIT MANUALLY]
[agent-doctor will update this section]
<!-- END PROTECTORATE COMMON -->

## Project-Specific Section
[manual content here - safe to edit]
```

For agents/skills/hooks - **Copy on sync**:
- Files copied from master to workspace .claude/
- Tracked via checksums or timestamps
- Overwrites on sync (master is authoritative)

**Implementation** (DECIDED: start as envoy tool):
- Initially: part of envoy binary, invoked via webui or API
- Future: extract to standalone if needed outside protectorate
- CLI: `envoy doctor sync [--dry-run] [workspace]`
- CLI: `envoy doctor diff [workspace]` - show what would change
- CLI: `envoy doctor init [workspace]` - set up a new workspace
- Optional: git hook to run on commit
- Optional: Envoy runs periodically

---

### Solution B: Plugin Inheritance (Sleeve Config)

Mount host's plugin directory into sleeves as read-only.

#### B.1: Volume Mount with Opt-Out (MVP)

```yaml
# In sleeve container spec
volumes:
  - ${HOME}/.claude/plugins:/home/sleeve/.claude/plugins:ro
  - ${HOME}/.claude/settings.json:/home/sleeve/.claude/settings.json:ro
```

**Spawn Modal Addition**:
```
[x] Inherit host plugins (default: checked)
    Mounts ~/.claude/plugins/ read-only into sleeve
```

**What to mount (read-only)**:
- `~/.claude/plugins/` - Installed plugins (includes hooks)
- `~/.claude/settings.json` - Plugin enablement settings
- `~/.claude/commands/` - User commands (if exists)
- `~/.claude.json` - User-level MCP servers (optional, separate checkbox?)

**What NOT to mount**:
- `.credentials.json` - Auth tokens (sleeves use own auth or none)
- `history.jsonl` - Session history
- `projects/` - Project-specific state
- Other runtime state

**Considerations**:
- Read-only prevents sleeves from modifying host plugins
- Sleeves cannot install new plugins (by design for MVP)
- If sleeve needs different plugins, user unchecks inheritance
- Hooks bundled in plugins will execute in sleeve context
- Hook scripts using `${CLAUDE_PLUGIN_ROOT}` should resolve correctly if plugins mounted at same path
- MCP servers requiring env vars need those vars passed to container

#### B.2: Envoy-Managed Distribution (Future)

Envoy becomes the central authority for sleeve configuration.

```
+------------------+
|      ENVOY       |
|  - Plugin cache  |
|  - Config store  |
|  - Version mgmt  |
+--------+---------+
         |
    +---------+----------+
    |         |          |
    v         v          v
 Sleeve A  Sleeve B  Sleeve C
 (plugins) (plugins) (no plugins)
```

**Capabilities**:
1. Envoy maintains canonical plugin cache
2. Envoy can exec into sleeves to install/update plugins
3. Per-sleeve configuration profiles
4. Version pinning and rollback
5. Plugin allowlist/blocklist per sleeve type

**Required Infrastructure** (not yet built):
- Envoy exec-into-container capability
- Plugin state tracking in envoy database
- Configuration sync protocol
- Webui for managing sleeve configurations

---

## Open Questions

### Agent Doctor
- [x] Where should master reference live? --> In protectorate repo (`agent-doctor/master/`)
- [x] Injection vs symlink vs copy? --> **Injection with markers** for CLAUDE.md, copy for agents/skills
- [ ] How to handle conflicts (manual edits to managed sections)?
- [x] Should agent-doctor be a separate binary or part of envoy? --> Start as envoy tool, separate later if needed
- [ ] Version tracking - how to know if workspace is out of sync?
- [ ] What goes in "common" vs stays project-specific? --> Criteria TBD after initial setup
      - Common: philosophies, tool usage, general rules (language-agnostic)
      - Project-specific: language-specific practices (Rust, Go, etc.), architecture docs

### Plugins (Solution B)
- [ ] Can plugins have sleeve-specific settings?
- [ ] How do plugin updates propagate to running sleeves?
- [ ] Should certain plugins be blocked in sleeves (security)?

### MCP Integrations
- [x] Where is MCP config stored? --> See findings below
- [ ] Are MCP servers per-user or per-project? --> Both options exist
- [ ] Do MCP servers need network access from sleeves?
- [ ] Should sleeves inherit user-level MCP config?
- [ ] How to handle MCP servers that need host-level access (e.g., Playwright)?

### Hooks
- [x] Where are hooks defined? --> Inside plugins (hooks/hooks.json)
- [ ] Should sleeve hooks differ from host hooks?
- [ ] Security implications of inherited hooks?
- [ ] Hooks that exec scripts - do paths resolve correctly in containers?

### Authentication
- [ ] Should sleeves share host credentials?
- [ ] Or should each sleeve have own API key?
- [ ] How to handle credential rotation?

### Multi-User
- [ ] If multiple users spawn sleeves, whose plugins?
- [ ] Protectorate-level plugin config vs user-level?

---

## Implementation Tasks

### Solution A: Agent Doctor

#### A.1: MVP (Envoy Tool)
- [ ] Create `agent-doctor/master/` directory structure in protectorate
- [ ] Create `agent-doctor/config.yaml` for workspace list
- [ ] Extract common sections from protectorate CLAUDE.md to `CLAUDE.md.common`
- [ ] Add injection markers to protectorate CLAUDE.md
- [ ] Implement `envoy doctor init [workspace]` - add markers to workspace CLAUDE.md
- [ ] Implement `envoy doctor sync [--dry-run] [workspace]` - inject/copy to workspaces
- [ ] Implement `envoy doctor diff [workspace]` - show pending changes
- [ ] Document usage in README

#### A.2: Integration (Future)
- [ ] Webui page for managing master reference
- [ ] Webui button to run sync
- [ ] Git hook integration - sync on commit
- [ ] Per-workspace overrides (skip certain files)
- [ ] Extract to standalone binary if needed outside protectorate

### Solution B: Plugin Inheritance

#### B.1: MVP (Volume Mounts)
- [ ] Add volume mounts to sleeve container spec
- [ ] Add "Inherit host plugins" checkbox to spawn modal
- [ ] Store preference in spawn request
- [ ] Update docker-compose and spawn logic
- [ ] Test plugin inheritance works
- [ ] Document in README

#### B.2: Envoy-Managed (Future)
- [ ] Design envoy plugin management API
- [ ] Implement exec-into-container for envoy
- [ ] Build plugin sync protocol
- [ ] Add webui plugin management page
- [ ] Per-sleeve configuration profiles

---

## Research Log

### 2026-01-27: Initial Exploration

**Findings**:
- Plugins installed at `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/`
- Plugin registry at `~/.claude/plugins/installed_plugins.json`
- Enabled plugins tracked in `~/.claude/settings.json` under `enabledPlugins`
- Plugins contain: agents/, skills/, hooks/, .claude-plugin/manifest.json

**Example plugin structure** (code-simplifier):
```
~/.claude/plugins/cache/claude-plugins-official/code-simplifier/1.0.0/
├── agents/
│   └── code-simplifier.md    # Agent definition
└── .claude-plugin/
    └── manifest.json         # Plugin metadata
```

**Host plugins installed**:
- code-simplifier@claude-plugins-official (v1.0.0)
- rust-analyzer-lsp@claude-plugins-official (v1.0.0)

### 2026-01-27: MCP and Hooks Deep Dive

**MCP Configuration Locations**:
```
OPTION 1: Project-level (recommended for teams)
<workspace>/.mcp.json    # Checked into git, shared with team

OPTION 2: User-level
~/.claude.json           # Global, applies to all projects
```

**MCP Config Example** (GitHub):
```json
{
  "github": {
    "type": "http",
    "url": "https://api.githubcopilot.com/mcp/",
    "headers": {
      "Authorization": "Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}"
    }
  }
}
```

**MCP Implications for Sleeves**:
- Project-level `.mcp.json` already inherited via workspace mount
- User-level `~/.claude.json` NOT inherited (need to decide)
- MCP servers requiring env vars (tokens) need those vars in container
- MCP servers needing local access (Playwright, filesystem) may not work in containers

**Hooks Architecture**:
- Hooks are PART OF plugins, not separate config
- Defined in `<plugin>/hooks/hooks.json`
- Hook events: PreToolUse, PostToolUse, Stop, UserPromptSubmit
- Hooks execute shell commands with timeout

**Example hooks.json** (hookify plugin):
```json
{
  "hooks": {
    "PreToolUse": [{
      "hooks": [{
        "type": "command",
        "command": "python3 ${CLAUDE_PLUGIN_ROOT}/hooks/pretooluse.py",
        "timeout": 10
      }]
    }],
    "PostToolUse": [...],
    "Stop": [...],
    "UserPromptSubmit": [...]
  }
}
```

**Hooks Implications for Sleeves**:
- Hooks come with plugins, so mounting plugins includes hooks
- `${CLAUDE_PLUGIN_ROOT}` must resolve correctly in container
- Hook scripts need their dependencies (python3, etc.) in container
- Security: inherited hooks run in sleeve context

### 2026-01-27: Configuration Ownership Clarification

**Key Realization**: Two separate problems exist:

1. **Sleeve isolation** - containers don't share `~/.claude/` (user-level)
2. **Workspace consistency** - project-agnostic config duplicated across repos

**Ownership Model**:
- `.claude/` and `CLAUDE.md` are GIT-CONTROLLED, not machine-dependent
- They travel with the repo, not the machine
- "Inheritance" problem only applies to `~/.claude/` (user-level)

**Agent Doctor Concept Introduced**:
- Tool to sync project-agnostic content across workspaces
- Master reference at `~/.protectorate/master/`
- Manages: common CLAUDE.md sections, shared agents, shared skills, shared hooks
- Does NOT manage: project architecture, project-specific agents/skills

**Sync Strategies Considered**:
1. Injection with markers (<!-- BEGIN/END PROTECTORATE COMMON -->)
2. Symlinks (always in sync, but git sees symlinks)
3. Copy on change (git-friendly, but can drift)

### 2026-01-27: Design Decisions Made

**Decisions**:
1. Master reference lives in protectorate repo at `agent-doctor/master/`
   - Keeps it versioned with protectorate
   - Can extract to separate repo later if needed for non-protectorate use
2. Sync strategy: **Injection** for CLAUDE.md, **Copy** for agents/skills/hooks
   - Injection preserves project-specific sections in CLAUDE.md
   - Copy is simpler for discrete files (agents, skills)
3. Implementation: Start as envoy tool (`envoy doctor ...`)
   - Leverage existing envoy infrastructure
   - Extract to standalone binary later if needed
4. Criteria for common vs project-specific: TBD after initial setup
   - Common = language-agnostic (philosophies, tool usage, general rules)
   - Project-specific = language-specific (Rust practices, Go idioms, etc.)

---

## Appendix: File Reference

### Plugin Manifest Example
```json
{
  "name": "code-simplifier",
  "version": "1.0.0",
  "description": "Agent that simplifies and refines code...",
  "author": {
    "name": "Anthropic",
    "email": "support@anthropic.com"
  }
}
```

### Settings.json Example
```json
{
  "enabledPlugins": {
    "code-simplifier@claude-plugins-official": true,
    "rust-analyzer-lsp@claude-plugins-official": true
  }
}
```

### Installed Plugins Registry Example
```json
{
  "version": 2,
  "plugins": {
    "code-simplifier@claude-plugins-official": [
      {
        "scope": "user",
        "installPath": "/home/user/.claude/plugins/cache/...",
        "version": "1.0.0",
        "installedAt": "2026-01-22T23:11:15.928Z"
      }
    ]
  }
}
```
