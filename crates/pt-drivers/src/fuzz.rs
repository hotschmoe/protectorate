//! The fuzz targets' driver: protocol mappers fed arbitrary harness lines and calls, with the
//! event-stream invariants the hub relies on checked after every step.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::emit::{MAX_BLOCK, Out};
use crate::event::{AckKind, Decision, Entry, Event, Harness, PromptKind, Stop};
use crate::plan::plan;
use crate::protocol::Protocol;
use crate::session;
use crate::spec::{Permission, SpawnSpec};

/// Drives a protocol mapper with arbitrary input. An optional first line `#` with
/// `resume=<id>` and `supervised` sets the spawn options. Lines starting with `!` are calls:
/// `!send [text]`, `!steer <turn>`, `!approve <id>`, `!session <id>`, `!deny <id>`,
/// `!cancel <id>`, `!interrupt`, `!compact`, and `!wrote`, which completes every stdin write so far
/// (until then a user message waits as the session's writer thread makes it wait). Every
/// other line goes through the same path as the harness's stdout. UUIDs are
/// `00000000-0000-4000-8000-000000000000`, so Claude's message n is `...-00000000000n`
/// (hex). Not a stable API.
pub fn fuzz_feed(harness: Harness, input: &[u8]) -> usize {
    let mut spec = SpawnSpec::new(harness, "harness", "/work");
    let mut lines = input.split(|b| *b == b'\n').peekable();
    let mut checked = false;
    if let Some(header) = lines.next_if(|l| l.first() == Some(&b'#')) {
        for word in String::from_utf8_lossy(&header[1..]).split_whitespace() {
            if let Some(id) = word.strip_prefix("resume=") {
                spec.resume = Some(id.to_owned());
            } else if word == "supervised" && harness != Harness::Pi {
                spec.permission = Permission::Supervised;
            } else if let Some(hash) = word.strip_prefix("check=") {
                let body = input
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(&[][..], |i| &input[i + 1..]);
                let actual = body.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                    (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
                });
                checked = u64::from_str_radix(hash, 16).ok() == Some(actual);
            }
        }
    }
    let mut zero = || Ok("00000000-0000-4000-8000-000000000000".to_owned());
    let Ok((_, mut proto)) = plan(&spec, &mut zero) else {
        return 0;
    };
    let mut out = Out::new(1);
    let mut check = Check::default();
    let mut inflight: Vec<String> = Vec::new();
    proto.start(&mut out);
    let mut n = 0;
    for (i, line) in lines.enumerate() {
        if let Some(call) = line.strip_prefix(b"!") {
            let call = String::from_utf8_lossy(call);
            let (verb, arg) = call.split_once(' ').unwrap_or((&call, ""));
            let req = format!("r{i}");
            let p = proto.as_mut();
            let text = if arg.is_empty() { "hi" } else { arg };
            let sent = match verb {
                "send" => {
                    session::deliver(p, &mut out, &req, "fuzz", PromptKind::Prompt, text, None)
                        .is_ok()
                }
                "steer" => session::deliver(
                    p,
                    &mut out,
                    &req,
                    "fuzz",
                    PromptKind::Prompt,
                    "hi",
                    Some(arg),
                )
                .is_ok(),
                "approve" => answer(p, &mut out, arg, Decision::AllowOnce),
                "session" => answer(p, &mut out, arg, Decision::AllowSession),
                "deny" => answer(p, &mut out, arg, Decision::Deny),
                "cancel" => answer(p, &mut out, arg, Decision::Cancel),
                "interrupt" => p.interrupt(&mut out).is_ok(),
                "compact" => p.compact(&mut out).is_ok(),
                "wrote" => {
                    for r in inflight.drain(..) {
                        out.written(&r);
                    }
                    false
                }
                "expect" => {
                    if checked && harness == Harness::Claude {
                        match arg {
                            "foreign-started" => assert_eq!(
                                (
                                    check.foreign_started,
                                    check.foreign_settled,
                                    check.confirmed.len()
                                ),
                                (1, 0, 0)
                            ),
                            "foreign-settled" => assert_eq!(
                                (
                                    check.foreign_started,
                                    check.foreign_settled,
                                    check.confirmed.len()
                                ),
                                (1, 1, 0)
                            ),
                            "echo-behind-foreign" => assert_eq!(
                                (
                                    check.foreign_started,
                                    check.foreign_settled,
                                    check.confirmed.len(),
                                    check.started.len()
                                ),
                                (1, 0, 1, 0)
                            ),
                            "echoed-turn-started" => assert_eq!(
                                (
                                    check.foreign_settled,
                                    check.confirmed.len(),
                                    check.started.len()
                                ),
                                (1, 1, 1)
                            ),
                            "foreign-interrupted" => assert_eq!(check.foreign_cancelled, 1),
                            "own-not-interrupted" => assert_eq!(check.own_cancelled, 0),
                            "prompt-confirmed" => assert_eq!(
                                (
                                    check.foreign_settled,
                                    check.confirmed.len(),
                                    check.own_settled
                                ),
                                (1, 1, 0)
                            ),
                            "own-settled" => assert_eq!(
                                (
                                    check.foreign_settled,
                                    check.confirmed.len(),
                                    check.own_settled
                                ),
                                (1, 1, 1)
                            ),
                            _ => panic!("unknown checked expectation {arg}"),
                        }
                    }
                    false
                }
                _ => false,
            };
            if sent && matches!(verb, "send" | "steer") {
                check.sent.insert(req, verb == "steer");
            }
        } else {
            session::handle_line(proto.as_mut(), &mut out, harness, session::parse_line(line));
        }
        for event in out.events.drain(..) {
            let text = pt_json::ToJson::to_json(&event).to_string();
            let back: Event = pt_json::from_str(&text).expect("an event parses back");
            assert_eq!(back, event, "an event survives a JSON round trip");
            n += text.len();
            check.event(&event);
        }
        for w in out.writes.drain(..) {
            n += w.line.to_string().len();
            inflight.extend(w.request);
        }
    }
    n
}

fn answer(p: &mut dyn Protocol, out: &mut Out, id: &str, d: Decision) -> bool {
    p.respond_approval(id, d, out).is_ok()
}

/// What the hub and core rely on in one session's event stream.
#[derive(Default)]
struct Check {
    last_seq: u64,
    /// Delivered requests, true for a steer.
    sent: HashMap<String, bool>,
    written: HashSet<String>,
    acked: HashSet<String>,
    confirmed: HashSet<String>,
    started: HashSet<String>,
    /// The prompt whose turn the next settle ends.
    current: Option<String>,
    turns: VecDeque<bool>,
    foreign_started: usize,
    foreign_settled: usize,
    own_settled: usize,
    foreign_cancelled: usize,
    own_cancelled: usize,
    /// Prompts whose turn settled with no turn_started for them.
    settled_unstarted: HashSet<String>,
    requested: HashSet<String>,
    pending: HashSet<String>,
}

impl Check {
    fn event(&mut self, event: &Event) {
        let text = matches!(
            event.entry,
            Entry::AssistantText { .. } | Entry::Reasoning { .. }
        );
        assert!(
            event.seq > self.last_seq || (text && event.seq == self.last_seq),
            "seq {} after {}",
            event.seq,
            self.last_seq
        );
        self.last_seq = event.seq;
        match &event.entry {
            Entry::AssistantText { text, .. } | Entry::Reasoning { text, .. } => {
                assert!(text.len() <= MAX_BLOCK + 64, "a text block is bounded");
            }
            Entry::UserMessage { request_id, .. } => {
                assert!(self.sent.contains_key(request_id), "a message we sent");
                assert!(
                    self.written.insert(request_id.clone()),
                    "one user_message per request"
                );
                if self.sent[request_id] {
                    return;
                }
                self.current = Some(request_id.clone());
            }
            Entry::PromptAck {
                request_id, ack, ..
            } => {
                assert!(
                    self.written.contains(request_id),
                    "user_message {request_id} precedes its ack"
                );
                assert!(self.acked.insert(request_id.clone()), "one ack per request");
                if *ack == AckKind::Confirmed {
                    assert!(
                        !self.settled_unstarted.contains(request_id),
                        "a confirmed prompt's turn settled without turn_started"
                    );
                    self.confirmed.insert(request_id.clone());
                }
            }
            Entry::TurnStarted {
                request_id: Some(r),
                ..
            } => {
                assert!(
                    self.written.contains(r),
                    "user_message {r} precedes its turn_started"
                );
                assert!(!self.sent[r], "a steer starts no turn");
                self.started.insert(r.clone());
                self.turns.push_back(false);
            }
            Entry::TurnStarted {
                request_id: None, ..
            } => {
                self.turns.push_back(true);
                self.foreign_started += 1;
            }
            Entry::TurnSettled { usage, stop, .. } => {
                for n in [
                    usage.input,
                    usage.output,
                    usage.cache_read,
                    usage.cache_write,
                ] {
                    assert!(n <= 1 << 40, "token counts are bounded");
                }
                let foreign = self.turns.pop_front().unwrap_or(false);
                if foreign {
                    self.foreign_settled += 1;
                    self.foreign_cancelled += usize::from(*stop == Stop::Cancelled);
                } else {
                    self.own_settled += 1;
                    self.own_cancelled += usize::from(*stop == Stop::Cancelled);
                }
                if !foreign
                    && let Some(r) = self.current.take()
                    && !self.started.contains(&r)
                {
                    assert!(
                        !self.confirmed.contains(&r),
                        "a confirmed prompt's turn settled without turn_started"
                    );
                    self.settled_unstarted.insert(r);
                }
                self.pending.clear();
            }
            Entry::ApprovalRequest { id, .. } => {
                assert!(
                    self.pending.insert(id.clone()),
                    "approval {id} requested twice while pending"
                );
                self.requested.insert(id.clone());
            }
            Entry::ApprovalDecision { id, .. } => {
                assert!(self.requested.contains(id), "a decision for a request");
                self.pending.remove(id);
            }
            _ => {}
        }
    }
}
