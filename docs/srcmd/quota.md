# Quota readers

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
The Zig hub does not yet call this selector when choosing a resleeve target.
Both requests have a 10-second deadline, a 5-second connection deadline, and a 2 MiB response
limit. Failure to fetch model quotas is an error; failure to fetch the tier leaves the quota
readings available with a note. The requests use Zig's in-process HTTP client.
