---
name: docker
description: Docker operations for Protectorate development
---

# /docker - Docker Operations

Manage Docker containers and images for Protectorate local development.

## Usage

```
/docker <command>
```

## Commands

| Command | Description |
|---------|-------------|
| `build` | Build all Docker images (envoy, sleeve) |
| `build envoy` | Build only the envoy image |
| `build sleeve` | Build only the sleeve image |
| `ps` | List running containers |
| `logs <container>` | View container logs |
| `network` | Create the cortical-net network |

## Examples

- `/docker build` - Build all images
- `/docker build sleeve` - Build sleeve image only
- `/docker ps` - List running containers
- `/docker logs envoy` - View envoy logs

## What It Does

1. Runs appropriate docker commands
2. Uses Dockerfiles in containers/ directory
3. Tags images with protectorate- prefix

## Implementation

```bash
# Build envoy image
docker build -f containers/envoy/Dockerfile -t protectorate-envoy .

# Build sleeve image
docker build -f containers/sleeve/Dockerfile -t protectorate-sleeve .

# Create network
docker network create cortical-net

# View logs
docker logs -f <container>

# List containers
docker ps --filter "name=protectorate"
```

## When to Use

- Building container images for testing
- Debugging container issues
- Managing the cortical-net network
