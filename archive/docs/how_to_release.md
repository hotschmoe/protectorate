# How to Release

This document describes the process for releasing a new version of Protectorate.

## Overview

Releases are automated via GitHub Actions. When you push a version tag, the workflow:

1. Builds all container images (base, envoy, sleeve)
2. Pushes them to GitHub Container Registry (ghcr.io)
3. Tags images with both the version and `latest`

## Prerequisites

- All changes merged to `master`
- `master` branch tested and working
- Git configured with push access to the repo

## Release Process

### 1. Ensure master is up to date

```bash
git checkout master
git pull origin master
```

### 2. Verify the build works locally

```bash
make release
make up
# Test the WebUI at http://localhost:7470
make down
```

### 3. Choose a version number

We use semantic versioning: `vMAJOR.MINOR.PATCH`

- **MAJOR**: Breaking changes (API, config format, etc.)
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, minor improvements

Check the latest release:
```bash
git tag --sort=-v:refname | head -5
```

### 4. Create and push the tag

```bash
# Create the tag
git tag v0.1.5

# Push the tag (triggers the release workflow)
git push origin v0.1.5
```

### 5. Monitor the workflow

1. Go to https://github.com/hotschmoe/protectorate/actions
2. Watch the "Release" workflow run
3. Verify all steps complete successfully

### 6. Verify the release

Check that images are available:
```bash
docker pull ghcr.io/hotschmoe/protectorate-envoy:v0.1.5
docker pull ghcr.io/hotschmoe/protectorate-sleeve:v0.1.5
```

Test the install script on a fresh machine:
```bash
curl -fsSL https://raw.githubusercontent.com/hotschmoe/protectorate/master/install.sh | bash
```

## What Gets Published

| Image | Registry Path |
|-------|---------------|
| Base | `ghcr.io/hotschmoe/protectorate-base:vX.X.X` |
| Envoy | `ghcr.io/hotschmoe/protectorate-envoy:vX.X.X` |
| Sleeve | `ghcr.io/hotschmoe/protectorate-sleeve:vX.X.X` |

All images are also tagged as `:latest`.

## Manual Workflow Trigger

If you need to rebuild without creating a new tag:

1. Go to Actions > Release workflow
2. Click "Run workflow"
3. Enter the existing tag (e.g., `v0.1.5`)
4. Click "Run workflow"

## Troubleshooting

### Workflow fails at "Build and push base image"

- Check the Dockerfile syntax in `containers/base/Dockerfile`
- Verify GHCR permissions in repo settings

### Workflow fails at "Build and push envoy/sleeve"

- The base image must build first
- Check that `BASE_IMAGE` and `BASE_TAG` ARGs are correct in Dockerfiles

### Images not accessible after release

- Packages may be private by default
- Go to repo Settings > Packages and make them public

### Install script can't pull images

- Wait a few minutes after the workflow completes
- Verify the package visibility is set to public
- Check the exact image name matches what the script expects

## Files Involved

| File | Purpose |
|------|---------|
| `.github/workflows/release.yaml` | GitHub Actions workflow |
| `containers/base/Dockerfile` | Base image build |
| `containers/envoy/Dockerfile` | Envoy image build |
| `containers/sleeve/Dockerfile` | Sleeve image build |
| `install.sh` | User-facing install script |
| `docker-compose.yaml` | Production compose (uses GHCR images) |
