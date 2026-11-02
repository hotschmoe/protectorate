//! The normalized transcript: one append-only stream of entries per incarnation
//! (docs/design/sessions-chat-first.md section 3).

use pt_json::Value;

pt_json::string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Harness {
        Claude = "claude",
        Codex = "codex",
        Grok = "grok",
        Pi = "pi",
    }

    /// What a user message is to the caller's turn. The driver records it as given.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PromptKind {
        Prompt = "prompt",
        /// The caller's continuation of a turn a crash ended.
        Continuation = "continuation",
        /// A resleeve's brief prompt.
        Brief = "brief",
    }

    /// What a tool does, for the collapsed work-group summary.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ToolKind {
        Command = "command",
        Read = "read",
        Edit = "edit",
        Search = "search",
        Web = "web",
        Mcp = "mcp",
        Subagent = "subagent",
        Other = "other",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ToolStatus {
        Completed = "completed",
        Failed = "failed",
        Declined = "declined",
        Cancelled = "cancelled",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Decision {
        AllowOnce = "allow_once",
        AllowSession = "allow_session",
        Deny = "deny",
        Cancel = "cancel",
    }

    /// Why a turn settled. `Other` keeps the harness's own word in `native_stop`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Stop {
        EndTurn = "end_turn",
        Cancelled = "cancelled",
        Error = "error",
        MaxTurns = "max_turns",
        Other = "other",
    }

    /// Read by guardrails: G1 quota and rate limits, G2 auth, G3 crash, G14 protocol.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorClass {
        Auth = "auth",
        Quota = "quota",
        RateLimit = "rate_limit",
        Protocol = "protocol",
        Crash = "crash",
        Other = "other",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AckKind {
        Confirmed = "confirmed",
        Rejected = "rejected",
    }
}

pt_json::record! {
    /// Token counts are what the harness reported for the turn; `context_tokens` is the
    /// prompt size of its last model call when known.
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct Usage {
        pub input: u64 = 0,
        pub output: u64 = 0,
        pub cache_read: u64 = 0,
        pub cache_write: u64 = 0,
        pub cost_usd: Option<f64>,
        pub context_tokens: Option<u64>,
        pub context_window: Option<u64>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct TouchedPath {
        pub path: String,
        pub op: Option<String>,
    }

    /// One entry. `seq` is monotonic per session; text deltas share the seq of the final
    /// entry of their block, so a store that keeps only finals has no gaps.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Event {
        pub seq: u64,
        pub g: u64,
        pub time_ms: u64,
        pub entry: Entry,
    }
}

pt_json::tagged_enum! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Entry {
        SessionInfo = "session_info" {
            harness: Harness,
            native_id: String,
            model: Option<String>,
            provider: Option<String>,
            effort: Option<String>,
            cwd: Option<String>,
        },
        /// Written to the harness. The protocol's verdict follows as `prompt_ack`.
        UserMessage = "user_message" {
            request_id: String,
            from: String,
            text: String,
            steer: bool = false,
            kind: PromptKind = PromptKind::Prompt,
        },
        PromptAck = "prompt_ack" {
            request_id: String,
            ack: AckKind,
            native_turn_id: Option<String>,
            detail: Option<String>,
        },
        /// `final` false is a live delta (not for storage); true carries the whole block.
        AssistantText = "assistant_text" {
            id: String,
            text: String,
            r#final: bool,
        },
        Reasoning = "reasoning" {
            id: String,
            text: String,
            r#final: bool,
        },
        ToolCall = "tool_call" {
            id: String,
            tool: ToolKind,
            name: String,
            summary: String,
            input: Option<Value>,
            truncated: bool = false,
        },
        ToolResult = "tool_result" {
            id: String,
            status: ToolStatus,
            output: String,
            truncated: bool = false,
            duration_ms: Option<u64>,
        },
        /// Paths the harness says a tool touched. The hub computes the diff from git.
        FileChange = "file_change" {
            tool_id: Option<String>,
            paths: Vec<TouchedPath> = Vec::new(),
        },
        ApprovalRequest = "approval_request" {
            id: String,
            tool: ToolKind,
            name: String,
            summary: String,
            input: Option<Value>,
            options: Vec<Decision> = Vec::new(),
        },
        /// `confirmed` is true only when the harness itself acknowledged the answer.
        ApprovalDecision = "approval_decision" {
            id: String,
            decision: Decision,
            by: Option<String>,
            confirmed: bool = false,
        },
        TurnStarted = "turn_started" {
            request_id: Option<String>,
            native_turn_id: Option<String>,
        },
        TurnSettled = "turn_settled" {
            stop: Stop,
            native_stop: Option<String>,
            usage: Usage,
            duration_ms: Option<u64>,
        },
        Error = "error" {
            class: ErrorClass,
            text: String,
        },
        Compaction = "compaction" {
            trigger: Option<String>,
            tokens_before: Option<u64>,
            tokens_after: Option<u64>,
        },
        /// `code` names the source: `unmapped:<type>` for a message type no mapping covers.
        Notice = "notice" {
            code: Option<String>,
            text: String,
        },
        /// The last entry of every session.
        Exited = "exited" {
            code: Option<i64>,
            signal: Option<i64>,
            requested: bool,
            stderr_tail: String,
        },
    }
}

/// Error class from a harness's own text and, when given, an HTTP status.
pub fn classify(text: &str, http_status: Option<u64>) -> ErrorClass {
    match http_status {
        Some(401 | 403) => return ErrorClass::Auth,
        Some(429) => return ErrorClass::RateLimit,
        Some(402) => return ErrorClass::Quota,
        _ => {}
    }
    let t = text.to_ascii_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if any(&[
        "usage limit",
        "quota",
        "out of credits",
        "insufficient credit",
        "insufficient_quota",
        "billing",
        "limit reached",
        "credit balance",
        "budget exceeded",
    ]) {
        ErrorClass::Quota
    } else if any(&[
        "rate limit",
        "rate_limit",
        "ratelimit",
        "too many requests",
        "429",
        "overloaded",
    ]) {
        ErrorClass::RateLimit
    } else if any(&[
        "not logged in",
        "/login",
        "log in",
        "unauthorized",
        "unauthenticated",
        "authentication",
        "invalid api key",
        "invalid x-api-key",
        "api key",
        "oauth",
        "token expired",
        "token has expired",
        "credentials",
        "401",
        "forbidden",
    ]) {
        ErrorClass::Auth
    } else {
        ErrorClass::Other
    }
}
