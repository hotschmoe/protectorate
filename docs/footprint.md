# Binary size and runtime memory

Build production binaries with `make build` or `make cross`. Both apply the
Tailscale feature tags printed by `make -s build-tags`. Release CI uses the
same targets; hub installs download those release assets, and node installers
and `node update` distribute the hub's matching `make cross` assets. There is
no separate node compiler or node executable format.

The selected omissions cover Tailscale's unused appliance, desktop, routing,
file-sharing, SSH, cloud authentication, telemetry and diagnostic integrations.
Protectorate implements its own jobs, file transfer, updates and diagnostics.
The tags do not remove preauth-key enrollment, WhoIs, LocalClient status,
DERP clients, our DERP server, STUN, netns, disco ping, userspace networking,
DNS, NAT port mapping, Windows or standalone Android/Termux support. System
proxy support and fallback certificate roots remain available.

ACME also remains linked: with the pinned Tailscale v1.104.1,
`ts_omit_acme` fails to compile because tsnet calls the omitted
`LocalClient.GetCertificate`. Do not add the tag without rechecking that
upstream constraint.
Its ACME refresh path also requires Serve, so `ts_omit_serve` is excluded.

When upgrading Tailscale, inspect its `feature/featuretags` registry and source
build constraints again, cross-build all five targets and run:

```sh
GOMAXPROCS=2 PROT_TAILNET_IT=1 go test -p 2 -timeout 15m \
  -tags "$(make -s build-tags)" -v ./internal/tailnet
make build
GOMAXPROCS=2 PROT_DIAGNOSTICS_BINARY="$PWD/bin/protectorate" go test -p 2 \
  -tags "$(make -s build-tags)" -run '^TestDiagnosticsEndToEnd$' ./cmd/protectorate
```

This includes `TestTailnetEndToEnd`, status/authorization checks and
`TestDERPRelay`, which forces relay traffic rather than accepting a direct
connection. It downloads and starts a temporary Headscale; it does not use
the running hub's state.

The diagnostics end-to-end test launches real hub and node processes with
temporary state, rejects broad scoped tokens from every debug endpoint,
collects owner profiles, refuses fallback-node diagnostics and verifies a
1 MiB job output stream byte for byte.

## Diagnostics

The hub serves `GET /debug/metrics`, `/debug/pprof/heap`,
`/debug/pprof/goroutine` and `/debug/pprof/profile` on its existing HTTP
listeners. Every debug request requires the owner bearer token. Scoped
tokens, including tokens with broad RPC grants, cannot read profiles.
Tailnet identity alone does not grant access; the normal tailnet admission
check also applies there. Responses use `Cache-Control: no-store`.

```sh
curl -fsS -H "Authorization: Bearer $(cat "$PROTECTORATE_HOME/token")" \
  http://127.0.0.1:7470/debug/metrics
curl -fsS -H "Authorization: Bearer $(cat "$PROTECTORATE_HOME/token")" \
  'http://127.0.0.1:7470/debug/pprof/heap?gc=1' -o heap.pprof
go tool pprof -top heap.pprof
curl -fsS -H "Authorization: Bearer $(cat "$PROTECTORATE_HOME/token")" \
  'http://127.0.0.1:7470/debug/pprof/profile?seconds=10' -o cpu.pprof
```

Use an explicit state directory in isolated profiling runs. Profiles can
contain sensitive process data. On nodes, the same paths are available at
port 7475 only on the enrolled tailnet listener and only to `tag:hub` callers,
verified with WhoIs. The unenrolled bearer-token fallback listener does not
enable diagnostics, even for a valid node token. Fetch node profiles with
the hub's owner-only proxy, which uses its tailnet HTTP client:

```sh
curl -fsS -H "Authorization: Bearer $(cat "$PROTECTORATE_HOME/token")" \
  http://127.0.0.1:7470/debug/nodes/NODE_NAME/metrics
curl -fsS -H "Authorization: Bearer $(cat "$PROTECTORATE_HOME/token")" \
  'http://127.0.0.1:7470/debug/nodes/NODE_NAME/pprof/heap?gc=1' -o node.pprof
```

The proxy accepts only inventory nodes and the four diagnostic paths. It
does not forward the owner token or follow redirects, and bounds each
request to two minutes. Use the hub's own paths for its local node.

The metrics response maps runtime/metrics names to integer values: total Go
memory, released heap, heap objects, stacks, live heap, heap goal, memory
limit, GC cycles and goroutine count. Total minus released approximates the
memory controlled by Go's soft limit; it is not process RSS. Heap `gc=1`
forces a GC and is useful for comparing retained objects; capture idle RSS
before that request to avoid confusing a forced collection with normal use.

## Soft memory limits

`node serve` defaults to a 128 MiB Go soft memory limit. This leaves room for
the embedded network stack and file/job streams on small machines while
asking Go to collect earlier during bursts. It excludes child job processes,
mapped executable pages and memory outside Go's control, and is not an OOM
guarantee. Job cgroup limits remain a separate setting.

Set `GOMEMLIMIT` using Go's syntax (for example `GOMEMLIMIT=192MiB`), or pass
`node serve --memory-limit 201326592`. An explicit flag takes precedence over
the environment; `GOMEMLIMIT=off` disables the default. The flag is a positive
byte count. Install commands pass
serve flags after `--`, so the same setting survives service installation.
The hub has no additional default: retained sessions, active streams and
concurrent jobs scale with the owner's workload. Set `GOMEMLIMIT` in its
service environment after measuring it rather than imposing a small node
budget on the hub.

## Measurements

Measurements use Go 1.27.2, `CGO_ENABLED=0`, `-trimpath -ldflags '-s -w'`,
and the same default version string on the base commit 028ea15 and this
change. Feature probes change one tag at a time against the base commit on
linux/arm64; combined savings are not the sum of individual savings because
features share code. The diagnostic handlers add some code back.

| Target | Before bytes | After bytes | Reduction |
| --- | ---: | ---: | ---: |
| linux/amd64 | 51,794,080 | 48,455,840 | 6.4% |
| linux/arm64 | 48,627,872 | 45,482,144 | 6.5% |
| windows/amd64 | 52,401,664 | 49,271,296 | 6.0% |
| windows/arm64 | 48,516,608 | 45,675,008 | 5.9% |
| android/arm64 | 52,560,166 | 50,266,406 | 4.4% |

RSS samples below are from separate real linux/arm64 processes with temporary
state and `GOMAXPROCS=2`, before forcing a profiling GC. The hub uses no
tailnet; the enrolled-node run uses a temporary real Headscale, DERP and STUN
instance and the hub's tsnet HTTP client. No running hub state is used.

| Process | Before RSS | After RSS |
| --- | ---: | ---: |
| Hub, no tailnet, 15 seconds idle | 47,760 KiB | 42,776 KiB |
| Node, token listener, 15 seconds idle | 37,156 KiB | 35,648 KiB |
| Node, enrolled tailnet, 20 seconds idle | 45,576 KiB | 44,408 KiB |

These are single-run observations, not a claim about the owner's production
hub. Idle RSS varies with GC and page residency. The enrolled node's metrics
reported the default limit as exactly 134,217,728 bytes, and its heap and
goroutine profiles were fetched successfully by `tag:hub`.
Separate enrolled-node runs reported 67,108,864 bytes with
`GOMEMLIMIT=64MiB`, and 100,663,296 bytes with that environment plus
`--memory-limit 100663296`, confirming the flag's precedence.

The actionable heap finding was `node.(*Jobs).Attach`: it allocated and held
a 1 MiB buffer per attached output stream, even before output arrived. It
now allocates 64 KiB on first output, matching the hub's log-reader size.
Upload and artifact frame sizes stay at 1 MiB. Smaller output frames also
bound the buffers retained by protobuf and HTTP compression.

A separate HTTP agent profiling harness starts 16 real plain shell jobs,
each running `head -c 1048576 /dev/zero; sleep 45`. After all output has been
read and while the streams remain open, RSS dropped from 72,480 KiB to
39,296 KiB; forced-GC sampled retained heap dropped from 34,686.78 KiB to
7,973.70 KiB. Direct sampled attachment allocations fell from 15,395.56 KiB
to 1,634 KiB. This harness includes its HTTP clients in the same process;
these figures describe the stream workload, not production node idle RSS.
The real-binary end-to-end test separately verifies complete output delivery.

The hub heap profile was mostly protobuf descriptors, API command/catalog
metadata and handler tables; there was no large idle buffer to trim locally.
No node-only binary is introduced: a second role-specific entry point would
also need enrollment, service installation, diagnostics and self-update CLI
support. The five existing install/update targets remain the release format.
