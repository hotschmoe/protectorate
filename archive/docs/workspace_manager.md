# Workspace Manager

The Workspace Manager handles all workspace operations in Protectorate, separating workspace management from sleeve lifecycle management.

## Architecture

```
+------------------+     +-------------------+     +----------------+
|    WebUI         |---->|  Envoy Server     |---->| Workspace      |
| (Workspaces Tab) |     |  /api/workspaces  |     | Manager        |
+------------------+     +-------------------+     +----------------+
                                                          |
                                                          v
                                                   +----------------+
                                                   | Filesystem     |
                                                   | /workspaces/   |
                                                   +----------------+
```

## Core Concepts

### Workspace
A directory on the filesystem that can be mounted into a sleeve. Workspaces are managed independently from sleeves, allowing:
- Pre-cloning repositories before spawning sleeves
- Reusing workspaces across multiple sleeve lifecycles
- Managing workspace state without affecting running sleeves

### Clone Job
An async operation that clones a git repository into a workspace. Jobs are tracked in memory and can be polled for status.

## API Endpoints

### List Workspaces
```
GET /api/workspaces
```

Returns all workspaces with their current status.

**Response:**
```json
[
  {
    "name": "my-project",
    "path": "/workspaces/my-project",
    "in_use": true,
    "sleeve_name": "quell"
  }
]
```

### Create Empty Workspace
```
POST /api/workspaces
Content-Type: application/json

{
  "name": "my-project"
}
```

Creates an empty workspace directory.

**Response:** `201 Created`
```json
{
  "name": "my-project",
  "path": "/workspaces/my-project",
  "in_use": false
}
```

### Clone Repository
```
POST /api/workspaces/clone
Content-Type: application/json

{
  "repo_url": "https://github.com/owner/repo",
  "name": "optional-workspace-name"
}
```

Starts an async clone operation. Returns immediately with a job ID for polling.

**Response:** `202 Accepted`
```json
{
  "id": "abc123def456",
  "repo_url": "https://github.com/owner/repo",
  "workspace": "/workspaces/repo",
  "status": "cloning",
  "start_time": "2024-01-15T10:30:00Z"
}
```

### Poll Clone Status
```
GET /api/workspaces/clone?id=abc123def456
```

Returns the current status of a clone job.

**Response:**
```json
{
  "id": "abc123def456",
  "repo_url": "https://github.com/owner/repo",
  "workspace": "/workspaces/repo",
  "status": "completed",
  "start_time": "2024-01-15T10:30:00Z",
  "end_time": "2024-01-15T10:30:45Z"
}
```

## Clone Job States

| Status | Description |
|--------|-------------|
| `cloning` | Clone operation in progress |
| `completed` | Clone finished successfully |
| `failed` | Clone failed (check `error` field) |

## Implementation Details

### WorkspaceManager (`internal/envoy/workspace_manager.go`)

```go
type WorkspaceManager struct {
    mu           sync.RWMutex
    cfg          *config.EnvoyConfig
    jobs         map[string]*protocol.CloneJob
    sleeveGetter func() []*protocol.SleeveInfo
}
```

**Key Methods:**
- `List()` - Returns all workspaces with in-use status
- `Create(name)` - Creates empty workspace directory
- `Clone(req)` - Starts async clone, returns job for polling
- `GetJob(id)` - Returns clone job status

### Job Lifecycle
1. Clone request received, job created with status `cloning`
2. Goroutine spawned to run `git clone`
3. On completion: status set to `completed` or `failed`
4. Jobs auto-expire from memory after 1 hour
5. Failed clones attempt to clean up partial directories

### Thread Safety
- All job map operations protected by `sync.RWMutex`
- Sleeve list fetched via callback to avoid circular dependencies
- Clone operations run in isolated goroutines

## Validation Rules

### Workspace Names
- Required for create operations
- Must not contain: `/`, `\`, `..`
- Pattern: `[a-zA-Z0-9_-]+`

### Repository URLs
- Must use HTTPS protocol
- Workspace name auto-derived from URL if not provided
- Example: `https://github.com/owner/repo` -> workspace name `repo`

## Error Handling

| Error | HTTP Status | Cause |
|-------|-------------|-------|
| workspace name required | 400 | Empty name in create request |
| invalid workspace name | 400 | Name contains invalid characters |
| workspace already exists | 400 | Directory already exists |
| repo_url required | 400 | Empty URL in clone request |
| only HTTPS URLs supported | 400 | Non-HTTPS URL provided |
| job not found | 404 | Invalid job ID in status poll |

## Future Enhancements

Planned features for the Workspace Manager:

### Workspace Operations
- [ ] Delete workspace (with safety checks for in-use)
- [ ] Rename workspace
- [ ] Archive/restore workspaces
- [ ] Workspace templates

### Git Operations
- [ ] Pull/fetch updates
- [ ] Branch switching
- [ ] Commit history view
- [ ] Diff view

### Clone Enhancements
- [ ] Clone progress reporting (percentage)
- [ ] Shallow clone option
- [ ] Specific branch/tag cloning
- [ ] SSH URL support (with key management)
- [ ] Private repository authentication

### Workspace Metadata
- [ ] Last modified timestamp
- [ ] Size on disk
- [ ] Git branch/status info
- [ ] Custom tags/labels

### Multi-Sleeve Support
- [ ] Read-only workspace sharing
- [ ] Workspace locking mechanisms
- [ ] Copy-on-write workspace clones
