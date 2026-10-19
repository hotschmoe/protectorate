# Protectorate

One pane of glass over your coding-agent subscriptions, machines, sessions and
work. A single Go binary is the hub, the node agent and the CLI.

## Ethos

- Let the harness do the work: we orchestrate vendor CLIs, never wrap or re-prompt them.
- Be as lightweight as possible: default to not building it.
- Everything is drivable from the CLI, with machine-readable `--json` output.

## Build

Needs Go, plus `buf`, `protoc-gen-go` and `protoc-gen-connect-go` (all via `go install`) to regenerate `gen/`.

```
make gen     # buf generate -> gen/ (committed)
make build   # bin/protectorate
make test    # go vet + go test -race
make cross   # linux/windows amd64+arm64, android/arm64 into bin/
```

## Run

```
protectorate hub serve                 # token created at ~/.protectorate/token
protectorate status
protectorate events                    # stream; --json for proto-JSON per line
protectorate api call HubService/GetStatus '{}'
curl -H "Authorization: Bearer $(cat ~/.protectorate/token)" \
     -H 'Content-Type: application/json' -d '{}' \
     http://127.0.0.1:7470/protectorate.v1.HubService/GetStatus
```

Flags: `--hub` (env `PROTECTORATE_HUB`), `--json`. State dir override: `PROTECTORATE_HOME`.

## Docs

- [docs/install.md](docs/install.md): hub, node and phone setup, service definitions
- [PROBLEMS.md](PROBLEMS.md): what hurts and why
- [ARCHITECTURE.md](ARCHITECTURE.md): the system we are building
