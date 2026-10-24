# MCP server

`protectorate mcp` (the CLI forwards to `protectorate-mcpbridge`) serves the
hub API as MCP tools so an AI client can act as, or talk to, the orchestrator.
It is a client of the hub: it finds its credential the way the CLI does and
calls the normal Connect API (`--hub` or `PROTECTORATE_HUB`, default
`http://127.0.0.1:7470`).

## Tools come from the proto

Nothing is hand-listed. The bridge reads the descriptor set with source
comments embedded in protectorate-wire
(`rust/crates/protectorate-wire/assets/protectorate-source.binpb`, written by
`make gen`) and turns every unary RPC into a tool:

- name: `<service>_<method>` in snake_case without the `Service` suffix, e.g.
  `quota_get_quota`, `backlog_create_note`
- description: the RPC's proto comment; field comments become property
  descriptions
- input schema: derived from the request message (enums as string enums,
  repeated, maps, nested messages, Timestamp, Duration, Struct)
- call: arguments are parsed as strict proto-JSON (unknown fields are an error),
  sent to the hub, and the response comes back as proto-JSON text
- errors: returned as tool errors whose text starts with the Connect code, e.g.
  `not_found: ...`

A new RPC in `proto/` appears as a tool after `make gen`; `make check-gen`
fails if the descriptor set or the tool catalog is stale.
Server-streaming RPCs (`EventService.Subscribe`) are not exposed; use
`protectorate events` for those.

## Scoping

An RPC marked `option idempotency_level = NO_SIDE_EFFECTS` is read-only; all
others are mutating and carry `readOnlyHint=false`. `(protectorate.v1.destructive)
= true` additionally sets `destructiveHint` (for example `KillSession`).

- No flags: read-only tools only.
- `--allow GLOB` (repeatable) replaces that default with globs over tool names.
  `--allow '*'` serves everything.
- `--deny GLOB` (repeatable) removes matches and always wins.

```
protectorate mcp                                   # read-only
protectorate mcp --allow 'backlog_*' --allow 'quota_*'
protectorate mcp --allow '*' --deny 'session_kill_*'
```

These filters only limit what the client is offered. What a bridge can actually
do is set by the credential it runs with, which the hub enforces. Give every
bridge its own capability (see [auth.md](auth.md)):

```
protectorate token create claude-bridge --scope read --scope 'backlog_*' --ttl 24h
protectorate mcp --token-file ~/.protectorate/claude-bridge.token --allow 'backlog_*'
```

The bridge reads the token file for every call and renews it in place past
half its life. Without a token file it uses `PROTECTORATE_TOKEN`, then the
device key `protectorate login` saved for the hub, then (on the hub host) the
owner token file. The HTTP endpoints need a token: their bearer is the token
the bridge started with. Scopes use the same tool names as this bridge. Only
`token_renew_token` of TokenService is a tool.

## Registering

stdio (default):

```
claude mcp add protectorate -- protectorate mcp
claude mcp add protectorate -- protectorate mcp --allow 'backlog_*'
codex mcp add protectorate -- protectorate mcp
```

Codex can also be configured in `~/.codex/config.toml`:

```
[mcp_servers.protectorate]
command = "protectorate"
args = ["mcp"]
```

Grok and pi: use their MCP server configuration with command `protectorate`
and args `["mcp"]` (the common `mcpServers` JSON shape). These two were not
verified against a live client.

Streamable HTTP, for bridges that cannot spawn a process:

```
protectorate mcp --http 127.0.0.1:7471 --token-file ~/.protectorate/claude-bridge.token
claude mcp add --transport http protectorate http://127.0.0.1:7471 \
  --header "Authorization: Bearer $(cat ~/.protectorate/claude-bridge.token)"
```

Every HTTP request must carry, as a bearer, the token the bridge read at
startup, or it gets 401. The bridge keeps renewing its token file for its own
hub calls, but the bearer it accepts stays the startup token. The endpoint acts with that token, so only a caller who already holds
it can use it; this reuses the existing token machinery instead of adding a
second secret. Requiring the bearer was chosen over only refusing non-loopback
binds because a loopback bind still lets any local process or user, and any
web page that can reach localhost, drive the bridge. Because the bearer is sent
in cleartext over plain HTTP, `--http` accepts only loopback: a private
address does not prove the path to it is encrypted, so any non-loopback
address, including an empty host, is refused.

For a remote MCP client, serve HTTPS with a certificate the client trusts:

```
protectorate mcp --https 192.168.22.5:7471 \
  --tls-cert /path/to/mcp-cert.pem --tls-key /path/to/mcp-key.pem \
  --token-file /path/to/scoped-bridge.token
```

The HTTPS endpoint still requires `Authorization: Bearer <token>` on every
request. `--https` takes an explicit private IP address (loopback, RFC 1918,
100.64.0.0/10, fc00::/7 or fe80::/10) and refuses wildcard and public
addresses and host names, like every other protectorate listener. Use
`https://HOST:7471` in the MCP client and configure it to trust
the certificate or its issuing CA. Keep the key file private (`chmod 600` on
Unix). On Windows, the key must be owned by the current user, SYSTEM, or
Administrators, and its ACL may grant access only to those principals. The
bridge rejects reparse paths and PEM files larger than 1 MiB. TLS handshakes
and live connections are bounded. `--http` and `--https` are mutually
exclusive. Windows Administrators and SYSTEM retain access to a private key;
the bridge cannot exclude them from a user-owned file.

HTTP bridges retain at most 128 sessions. Clients should close their session
with `DELETE` when finished; a full bridge returns HTTP 503 for a new session
while existing clients keep working. Sessions idle for an hour are expired
on the next request. Active requests are allowed to finish.
Each session admits at most 128 requests in progress, on stdio and HTTP;
excess requests receive JSON-RPC error -32000. Cancellation notifications
remain available at capacity and release slots when their calls finish.

## Example tool list (`--allow '*'`)

`protectorate mcp --list` prints the catalog the bridge would serve, with the
same `--allow`/`--deny` filters, as JSON and exits without contacting the hub.
`make gen` writes
`rust/crates/protectorate-mcpbridge/tests/testdata/mcp_tools.json` from it,
and the bridge's e2e scenario checks the served `tools/list` against that
file, so a schema change updates the golden like any other generated output.

```
backlog_create_note     mutating
backlog_delete_note     mutating, destructive
backlog_get_note        read-only
backlog_list_notes      read-only
backlog_update_note     mutating
hub_get_status          read-only
quota_get_quota         read-only
quota_suggest           read-only
session_capture_pane    read-only
session_kill_session    mutating, destructive
session_list_sessions   read-only
```
