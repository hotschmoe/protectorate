# Quota readers and target selection

Spawn defaults to Claude. `--harness auto` or Auto in the app opts into quota selection across
Claude, Codex, Grok and local Pi. An explicitly supplied model or provider uses the requested
harness or Claude. Envoy defaults to Claude and uses a fixed harness. Manual resleeves specify
their target and inherit launch
settings only from that harness.

Quota and auth guardrails rank the sleeve's configured alternatives, excluding its current
harness. The sleeve parks if there is no usable alternative. Automatic candidates must be
eligible according to the harness checks shared with `doctor`. `select_with_fallback` first
ranks fresh readings, then tries usable stale rows, then reset or unknown rows in candidate
order. Rows age out of the fresh pass after ten minutes.

Set `"quota_local_provider": "<provider>"` in the hub's `config.json` to identify a free Pi
provider in the hub user's `~/.pi/agent/models.json`. The hub checks that provider's endpoint and
passes its first configured model ID and provider name when it selects local Pi. Other custom Pi
endpoints are outside local quota selection.

Auto spawn and guardrail outcomes appear in `protectorate event list` after journal replay.

The code takes the median USD cost of up to nine recent successful turns with reported costs in
the root workspace and its descendants. A median below $0.05 is small, below $0.50 is medium,
and the rest are large. A workspace without measured costs starts at medium. An owner can set
`--task-size small|medium|large` for a request. Envoy's requested class is clamped to at most
one class away from the measured class.

## Antigravity reader

The Antigravity reader reads `~/.gemini/antigravity-cli/antigravity-oauth-token` under the
hub user's home. Its nested `token` object contains `access_token`, RFC 3339 `expiry`, and a
refresh token. Protectorate neither writes the file nor refreshes the token. An expired access
token with a refresh token is stale until agy next runs; an expired access token without a refresh
token, or a fresh token rejected with HTTP 401, is an expired login. A missing or invalid expiry
is an error. Missing credentials are reported as not configured. Across empty stale reads for any
subscription, the hub retains the last windows, plan, credits and original fetch time for at most
24 hours, dropping each window as soon as its reset time passes. Windows without a reset time are
also subject to the 24-hour limit. Once no window remains, the stale reading carries no old data.

With the access token, the reader POSTs `{}` to the same Cloud Code Assist internal endpoints
observed in agy's log: `v1internal:fetchAvailableModels` and `v1internal:loadCodeAssist` at
`https://daily-cloudcode-pa.googleapis.com`. The first returns a `models` object keyed by model
ID; entries can carry `quotaInfo.remainingFraction` and `quotaInfo.resetTime`. The reader
reports each model with a valid fraction as a separate scope, converting remaining to
percent used. It leaves the window duration unknown and does not infer pools from equal values.
The second returns `paidTier` and `currentTier`; the paid tier name is preferred. The observed
account returned Google AI Ultra in `paidTier` and Antigravity in `currentTier`. Windows are sorted
by highest usage first, with model ID as the tie breaker. The CLI and app show at most six, with
the number of remaining models; the API retains all of them for model-specific selection.

For a specified model, selection matches model IDs that begin with the sleeve's model name after
folding case and punctuation, and uses the tightest matching window. For an unspecified or default
model, it uses the model with the most remaining quota, subject to any shared windows. Models
without valid quota fractions are omitted; a specified model with no matching reading is unknown.
Both requests have a 10-second deadline, a 5-second connection deadline, and a 2 MiB response
limit. Failure to fetch model quotas is an error; failure to fetch the tier leaves the quota
readings available with a note. The requests use Zig's in-process HTTP client.
