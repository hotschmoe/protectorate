# Testing policy

The suite has two kinds of test and nothing else:

- **End-to-end scenarios** that run shipped binaries (`CARGO_BIN_EXE_*`), a
  real hub or node over its network API, or real external processes (the
  namespace sandbox helper, tmux, git, the Flutter tool scenarios), and that
  would fail on a user-visible regression.
- **Fuzz targets** over parsers that read untrusted or on-disk input.

No unit tests, no `#[cfg(test)]` modules in `src/`, no fakes or mocks of the
thing under test, no tests of help text, serialization shape or internals.
Deterministic executables speaking a harness's official protocol are allowed
as peers; installed AI CLIs are never wrapped or patched. Tests never use the
running hub, the owner's logins or provider credentials.

There is no backward compatibility to test: hubs and nodes are reinstalled
fresh, so no test exercises old Go formats, legacy fields or state migration.

How to run each lane: [testing.md](testing.md).
