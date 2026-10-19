# Configuration Simplification TODO

Future improvements to make Protectorate more robust and user-friendly.

---

## Config Source Consolidation

**Current state:** Two config sources
- `configs/envoy.yaml` - Static configuration
- `.env` - Environment variables (secrets, deployment-specific)

**Problem:** Users must understand which values go where. Some values are duplicated or have confusing precedence.

**Proposed solution:**

1. **Hardcode defaults in Go binary**
   - All sensible defaults baked into `internal/config/config.go`
   - No config file required for basic operation
   - `envoy.yaml` becomes optional (for advanced overrides only)

2. **Use `.env` for everything user-configurable**
   - Secrets (tokens, API keys)
   - Deployment-specific paths
   - Image overrides (for dev)

3. **Eliminate `envoy.yaml` for most users**
   - Advanced users can still use it
   - Document it as "advanced configuration"

**Result:** Single source of truth (`.env`), simpler mental model.

---

## Installer Improvements

### Short-term

- [ ] Add `--dry-run` flag to show what would be done
- [ ] Add `--version` flag to install specific version
- [ ] Add `--no-start` flag to skip starting envoy
- [ ] Better error messages with suggested fixes
- [ ] Detect and warn about port 7470 conflicts

### Medium-term

- [ ] macOS support (Docker Desktop detection)
- [ ] WSL2 support (Windows users)
- [ ] Uninstall script (`uninstall.sh`)
- [ ] Update command (`protectorate update` or re-run installer)
- [ ] Health check after install with troubleshooting tips

### Long-term

- [ ] TUI installer using `gum` or `whiptail` for better UX
- [ ] Interactive mode vs non-interactive (CI) mode
- [ ] Systemd service installation (auto-start on boot)
- [ ] Custom domain for installer URL (protectorate.dev/install.sh)

---

## Release Process Improvements

### Automated

- [ ] GitHub Release notes auto-generated from commits
- [ ] Changelog generation (keep a CHANGELOG.md)
- [ ] Version bumping script
- [ ] Release checklist in PR template

### Versioning

- [ ] Semantic versioning (vMAJOR.MINOR.PATCH)
- [ ] Pre-release tags (v0.1.0-alpha, v0.1.0-rc1)
- [ ] Document breaking changes policy

---

## Container Image Improvements

### Size Optimization

- [ ] Multi-stage builds already in place (good)
- [ ] Consider Alpine base for smaller images
- [ ] Audit installed packages in base image
- [ ] Layer caching optimization

### Security

- [ ] Run as non-root user (already done for claude user)
- [ ] Scan images for vulnerabilities (trivy in CI)
- [ ] Sign images (cosign)
- [ ] SBOM generation

### Registry

- [ ] Consider Docker Hub mirror for discoverability
- [ ] Image retention policy (keep last N versions)
- [ ] Multi-arch builds (amd64, arm64)

---

## User Experience Improvements

### Documentation

- [ ] Quick start guide (30-second version)
- [ ] Troubleshooting guide with common issues
- [ ] Architecture diagram for README
- [ ] Video walkthrough of installation

### CLI

- [ ] `protectorate` CLI wrapper for common operations
  - `protectorate status` - Show envoy and sleeve status
  - `protectorate logs` - Tail envoy logs
  - `protectorate spawn` - Spawn a sleeve
  - `protectorate stop` - Stop all sleeves

### Web UI

- [ ] Installation status page (first-run wizard)
- [ ] Health dashboard with system requirements check
- [ ] One-click sleeve spawn with presets

---

## Error Handling

### Installer

- [ ] Graceful handling of network failures
- [ ] Retry logic for transient errors
- [ ] Rollback on partial failure
- [ ] Clear error codes with documentation links

### Runtime

- [ ] Better error messages in envoy logs
- [ ] Structured logging (JSON option)
- [ ] Error reporting endpoint (opt-in telemetry)

---

## Testing

### Installer

- [ ] Test in Docker container (clean environment)
- [ ] Test on multiple distros (Ubuntu, Debian, Fedora)
- [ ] Test upgrade path (v0.1.0 -> v0.2.0)
- [ ] Integration test in CI

### Images

- [ ] Container structure tests
- [ ] Smoke test after build (health endpoint responds)
- [ ] Size regression tests

---

## Priority Order

1. **High Impact, Low Effort**
   - Hardcode defaults in Go (config consolidation)
   - Add `--version` flag to installer
   - Basic troubleshooting in README

2. **High Impact, Medium Effort**
   - `protectorate` CLI wrapper
   - macOS support
   - Vulnerability scanning in CI

3. **Nice to Have**
   - TUI installer
   - Multi-arch builds
   - Video documentation

---

## Notes

- "Make it work, make it right, make it fast" - We're in "make it work" phase
- Avoid premature optimization
- User feedback should drive prioritization
- Keep the core simple, add complexity through optional features
