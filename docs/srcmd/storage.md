# Persistent Storage (`src/journal/`, `src/history/`, `src/update/`)

## Segmented Event Journal (`src/journal/`)

Every state mutation is serialized canonically (`core/codec.zig`), wrapped in a length-prefixed CRC32C frame (`journal/codec.zig`), written positionally to `segment-000000000001` under an exclusive advisory file lock (`journal/store.zig`), and `fsync`ed before mutating external systems. Segments rotate automatically at 64 MiB (`max_segment_bytes`).

Recovery distinguishes between crash-time incomplete tails and true disk corruption:
- **Torn tail on the last segment** (`stop == .truncated` or last-frame CRC mismatch with no intact subsequent frames): truncated cleanly via `setLength` to the last valid prefix.
- **Mid-log or non-final segment corruption** (CRC mismatch with an intact frame after it, or corruption on an earlier segment): refuses to start with `error.JournalCorrupted`.

## BM25 History Index (`src/history/`)

`history/index.zig` incrementally tails native CLI session files (`~/.claude/projects`, `~/.codex/sessions`, `~/.pi/agent/sessions`, `~/.grok`) and hub transcripts (`<home>/transcripts/*.jsonl`). Turns are normalized via `history/extract.zig` and indexed into immutable `.ptseg` binary segments (`PTHISTSG` header + `PTHX` CRC32C footer, `history/segment.zig`). Non-overlapping field token positions with explicit `+1` gaps prevent spurious phrase matches across text/file boundaries. Search queries rank with BM25 (`k1=1.2, b=0.75`) and synthesize highlighted 240-byte snippets.

## Self-Update Store (`src/update/`)

`update/store.zig` fetches release metadata and assets directly over `std.http.Client`, verifies cryptographic `SHA256SUMS` digests via `update/version.zig`, unpacks `.tar.gz` archives purely in-process (`std.compress.flate.Decompress` + `std.tar.extract`), swaps `current`/`previous` links, and supports probation health markers and rollback without spawning `curl`, `tar`, or `gzip`.
