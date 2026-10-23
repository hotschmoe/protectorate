//! Prompt-cache telemetry derived from the usage events a log carries.
use serde_json::{json, Value};

#[derive(Clone, Copy, Default)]
struct Counts {
    calls: u64,
    input: u64,
    output: u64,
    read: u64,
    write: u64,
}

fn count(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_default()
}

impl Counts {
    fn of(usage: &Value) -> Self {
        Self {
            calls: 1,
            input: count(&usage["inputTokens"]),
            output: count(&usage["outputTokens"]),
            read: count(&usage["cacheReadTokens"]),
            write: count(&usage["cacheWriteTokens"]),
        }
    }
    fn add(&mut self, other: Self) {
        self.calls += other.calls;
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.read = self.read.saturating_add(other.read);
        self.write = self.write.saturating_add(other.write);
    }
    fn json(self) -> Value {
        let prompt = self
            .input
            .saturating_add(self.read)
            .saturating_add(self.write);
        let ratio = if prompt == 0 {
            0.0
        } else {
            self.read as f64 / prompt as f64
        };
        json!({"calls":self.calls.to_string(),"inputTokens":self.input.to_string(),
            "outputTokens":self.output.to_string(),"cacheReadTokens":self.read.to_string(),
            "cacheWriteTokens":self.write.to_string(),"readRatio":ratio})
    }
}

/// Totals over the whole log, the running turn, and the first model call of
/// the current harness run. A run that starts on a log with earlier calls is
/// a resume, where that first call is expected to read the cache.
#[derive(Default)]
pub(crate) struct Telemetry {
    total: Counts,
    turn: Counts,
    first: Option<Counts>,
    resumed: bool,
}

impl Telemetry {
    pub(crate) fn begin_run(&mut self) {
        self.first = None;
        self.resumed = self.total.calls > 0;
    }

    /// Folds one event in. A settled turn is annotated with its totals.
    pub(crate) fn observe(&mut self, payload: &mut Value) {
        if payload.get("usage").is_some() {
            let call = Counts::of(&payload["usage"]);
            self.total.add(call);
            self.turn.add(call);
            self.first.get_or_insert(call);
        } else if payload.get("user").is_some() {
            self.turn = Counts::default();
        } else if payload.get("handoff").is_some() {
            self.first = None;
            self.resumed = false;
        } else if let Some(settled) = payload.get_mut("settled").and_then(Value::as_object_mut) {
            if self.turn.calls > 0 {
                settled.insert("usage".into(), self.turn.json());
            }
            self.turn = Counts::default();
        }
    }

    pub(crate) fn json(&self) -> Value {
        let mut cache = json!({"total":self.total.json(),"resumed":self.resumed});
        if let Some(first) = self.first {
            cache["firstCall"] = first.json();
            cache["coldResume"] = json!(
                self.resumed && first.read == 0 && first.input.saturating_add(first.write) > 0
            );
        }
        cache
    }
}
