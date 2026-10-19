# TODO: CLI vs WebAPI / Envoy Container vs Native Binary

### Current State
- Envoy runs inside a Docker container
- Doctor (and other features) are webui-only
- API exists at `/api/doctor` but no CLI wrapper

### Questions to Discuss

**Should we add `envoy --doctor` CLI?**
- Currently users would need: `docker exec envoy-poe envoy --doctor`
- Or use the API: `curl -s localhost:7470/api/doctor | jq`
- Is CLI convenience worth the added code?

**Should envoy run on host instead of in container?**

Arguments for native binary on host:
- Direct access to Docker socket (no socket mounting)
- CLI tools like `envoy --doctor` work naturally
- Simpler debugging
- Can manage SSH agent, git config directly
- No container-in-container complexity

Arguments for containerized envoy:
- Consistent environment across platforms
- Single `docker compose up` to run everything
- No Go build required on user's machine
- Isolation from host system
- Easier updates (just pull new image)

**Hybrid approach?**
- Thin CLI wrapper on host that calls containerized envoy API
- Best of both worlds but more moving parts

### Decision Needed
This affects architecture significantly. Worth dedicated discussion.
