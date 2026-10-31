//! Scenario scripts: the committed seeds of `sleeve_core`.
//!
//! A script starts with `#scenario <hash>`, where `<hash>` is the FNV-1a 64 of every byte after
//! that first line, in lowercase hex. Its `expect` lines are checked only when the hash matches,
//! so libFuzzer's mutations of a seed still run as event sequences but never trip a stale
//! expectation. Every line drives the same simulated hub as the byte-driven mode, so the
//! invariants and the replay check apply to scenarios too.
//!
//! ```text
//! spawn NAME PATH [parent=NAME] [view] [harness=H] [pane] [budget=N] [disk=N] [to=H,H]
//! send NAME ID [steer] [from=NAME]
//! stop|remove|interrupt|compact|viewed NAME
//! start|resleeve NAME [harness=H]
//! approve NAME ID allow|deny        abandon KEY
//! obs NAME WHAT [g=N]               WHAT: ready progress[=H] approval=ID resolved=ID
//!                                         settled[=usage|auth|interrupted|error] compactdone
//!                                         desync screen=V usage=N disk=N forbidden
//! exit NAME [oom] [code=N]          the incarnation's process tree dies
//! tick DURATION                     e.g. 30s 16m 2h
//! policy KEY=VALUE...               see `Policy`
//! run | step | commit | crash [empty]
//! job ID | cancel-job N | requeue-job N | abandon-job N
//! expect NAME life=.. g=N activity=.. guard=.. harness=.. queue=N msg:ID=..
//! expect-effect KEY state=.. attempt=N count=N
//! expect-job N state=..           expect-none NAME
//! ```

use pt_core::*;

use crate::sim::{Policy, Sim};

pub fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Runs a script. Returns false if `data` is not a script.
pub fn run(data: &[u8]) -> bool {
    let Some(rest) = data.strip_prefix(b"#scenario ") else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return true;
    };
    let (head, body) = text.split_once('\n').unwrap_or((text, ""));
    let checked = u64::from_str_radix(head.trim(), 16).ok() == Some(fnv(body.as_bytes()));
    let mut r = Runner {
        sim: Sim::new(),
        policy: Policy::default(),
        checked,
    };
    for (n, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if r.sim.steps > 5000 {
            break;
        }
        if let Err(e) = r.line(line) {
            assert!(!checked, "scenario line {}: {e}: {line}", n + 2);
            break;
        }
    }
    r.sim.finish();
    true
}

struct Runner {
    sim: Sim,
    policy: Policy,
    checked: bool,
}

fn harness(s: &str) -> Result<Harness, String> {
    Ok(match s {
        "claude" => Harness::Claude,
        "codex" => Harness::Codex,
        "grok" => Harness::Grok,
        "antigravity" => Harness::Antigravity,
        "pi_vendor" => Harness::PiVendor,
        "pi_local" => Harness::PiLocal,
        "pi_durable" => Harness::PiDurable,
        _ => return Err(format!("unknown harness {s}")),
    })
}

fn target(h: Harness, pane: bool) -> Target {
    Target {
        harness: h,
        mode: if pane { Mode::Pane } else { Mode::Headless },
        provider: "default".into(),
        model: "default".into(),
    }
}

fn duration(s: &str) -> Result<Millis, String> {
    let (n, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = n.parse().map_err(|_| format!("bad duration {s}"))?;
    Ok(n * match unit {
        "s" => SECOND,
        "m" => MINUTE,
        "h" => HOUR,
        _ => return Err(format!("bad duration {s}")),
    })
}

/// The variant name of a Debug-printed value: `Stopping { remove: false }` is `Stopping`.
fn variant(v: &impl std::fmt::Debug) -> String {
    let s = format!("{v:?}");
    s.split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("")
        .to_string()
}

fn num(s: &str) -> Result<u64, String> {
    s.parse().map_err(|_| format!("bad number {s}"))
}

impl Runner {
    fn id(&self, name: &str) -> Result<SleeveId, String> {
        self.sim
            .st
            .sleeves
            .values()
            .filter(|s| s.name == name)
            .map(|s| s.id)
            .next_back()
            .ok_or_else(|| format!("no sleeve {name}"))
    }

    fn effect(&self, key: &str) -> Result<EffectId, String> {
        self.sim
            .st
            .effects
            .values()
            .filter(|e| e.key == key)
            .map(|e| e.id)
            .next_back()
            .ok_or_else(|| format!("no effect {key}"))
    }

    fn line(&mut self, line: &str) -> Result<(), String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let kv = |k: &str| {
            words
                .iter()
                .find_map(|w| w.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
        };
        let flag = |f: &str| words.contains(&f);
        let arg = |i: usize| words.get(i).copied().ok_or("missing argument".to_string());
        use EventKind as E;
        match words[0] {
            "spawn" => {
                let parent = kv("parent").map(|p| self.id(p)).transpose()?;
                let view = flag("view");
                let h = harness(kv("harness").unwrap_or("claude"))?;
                let to = match kv("to") {
                    Some(list) => list
                        .split(',')
                        .map(|h| harness(h).map(|h| target(h, false)))
                        .collect::<Result<_, _>>()?,
                    None => vec![target(Harness::PiLocal, false)],
                };
                let spec = SpawnSpec {
                    request_id: format!("spawn-{}", arg(1)?),
                    name: arg(1)?.into(),
                    parent,
                    layout: match (parent, view) {
                        (None, _) => Layout::Root,
                        (Some(_), false) => Layout::Worktree,
                        (Some(_), true) => Layout::View,
                    },
                    claim: Claim {
                        path: arg(2)?.into(),
                        writable: !view,
                    },
                    target: target(h, flag("pane")),
                    resleeve_to: to,
                    budget: kv("budget").map(num).transpose()?,
                    disk_limit: kv("disk").map(num).transpose()?,
                };
                self.sim.apply(E::Spawn(spec));
            }
            "send" => {
                let to = self.id(arg(1)?)?;
                let from = match kv("from") {
                    Some(n) => Party::Sleeve(self.id(n)?),
                    None => Party::Owner,
                };
                let id = arg(2)?;
                self.sim.apply(E::Send {
                    to,
                    request_id: id.into(),
                    from,
                    text: format!("text of {id}"),
                    mode: if flag("steer") {
                        SendMode::Steer
                    } else {
                        SendMode::Queue
                    },
                });
            }
            "stop" | "remove" | "interrupt" | "compact" | "viewed" | "start" | "resleeve" => {
                let sleeve = self.id(arg(1)?)?;
                let t = kv("harness")
                    .map(harness)
                    .transpose()?
                    .map(|h| target(h, false));
                self.sim.apply(match words[0] {
                    "stop" => E::Stop { sleeve },
                    "remove" => E::Remove { sleeve },
                    "interrupt" => E::Interrupt { sleeve },
                    "compact" => E::Compact { sleeve },
                    "viewed" => E::Viewed { sleeve },
                    "start" => E::Start { sleeve, target: t },
                    _ => E::Resleeve { sleeve, target: t },
                });
            }
            "approve" => {
                let sleeve = self.id(arg(1)?)?;
                self.sim.apply(E::RespondApproval {
                    sleeve,
                    approval: arg(2)?.into(),
                    allow: arg(3)? == "allow",
                    note: String::new(),
                });
            }
            "abandon" => {
                let effect = self.effect(arg(1)?)?;
                self.sim.apply(E::Abandon { effect });
            }
            "obs" => {
                let sleeve = self.id(arg(1)?)?;
                let g = match kv("g") {
                    Some(n) => Gen(num(n)?),
                    None => self.sim.st.sleeves[&sleeve].g,
                };
                let what = arg(2)?;
                let (w, v) = what.split_once('=').unwrap_or((what, ""));
                let obs = match w {
                    "ready" => Obs::Ready {
                        session: format!("session-{}", g.0),
                    },
                    "progress" => Obs::Progress {
                        tool: if v.is_empty() { None } else { Some(num(v)?) },
                    },
                    "approval" => Obs::ApprovalRequested { id: v.into() },
                    "resolved" => Obs::ApprovalResolved { id: v.into() },
                    "settled" => Obs::TurnSettled {
                        end: match v {
                            "" | "done" => TurnEnd::Done,
                            "usage" => TurnEnd::UsageLimit,
                            "auth" => TurnEnd::AuthFailed,
                            "interrupted" => TurnEnd::Interrupted,
                            "error" => TurnEnd::Error,
                            _ => return Err(format!("bad turn end {v}")),
                        },
                    },
                    "compactdone" => Obs::CompactDone,
                    "desync" => Obs::Desync {
                        detail: "oversize frame".into(),
                    },
                    "screen" => Obs::Screen(match v {
                        "idle" => Verdict::Idle,
                        "working" => Verdict::Working,
                        "blocked" => Verdict::Blocked,
                        "nomatch" => Verdict::NoMatch,
                        _ => return Err(format!("bad verdict {v}")),
                    }),
                    "usage" => Obs::Usage { tokens: num(v)? },
                    "disk" => Obs::Disk { bytes: num(v)? },
                    "forbidden" => Obs::ForbiddenPath {
                        path: ".git/hooks/pre-commit".into(),
                    },
                    _ => return Err(format!("bad observation {what}")),
                };
                self.sim.observe(sleeve, g, obs);
            }
            "exit" => {
                let sleeve = self.id(arg(1)?)?;
                let g = self.sim.st.sleeves[&sleeve].g;
                let cause = if flag("oom") {
                    ExitCause::Oom
                } else {
                    ExitCause::Exit(kv("code").map(num).transpose()?.unwrap_or(1) as i32)
                };
                self.sim.crash_process(sleeve, g, cause);
            }
            "tick" => self.sim.tick(duration(arg(1)?)?),
            "policy" => {
                for w in &words[1..] {
                    let (k, v) = w.split_once('=').ok_or(format!("bad policy {w}"))?;
                    let pick = |names: &[&str]| {
                        names
                            .iter()
                            .position(|n| *n == v)
                            .map(|i| i as u8)
                            .or_else(|| v.parse().ok())
                            .ok_or(format!("bad policy value {w}"))
                    };
                    let p = &mut self.policy;
                    match k {
                        "start" => p.start = pick(&["ok", "fail", "lostreceipt", "lost"])?,
                        "prompt" => p.prompt = pick(&["ack", "nack", "lost", "none"])?,
                        "stop" => p.stop = pick(&["exit", "linger"])?,
                        "other" => p.other = pick(&["ack", "nack", "lost"])?,
                        "probe" => p.probe = pick(&["applied", "notapplied", "unknowable"])?,
                        "select" => p.select = pick(&["first", "none", "second"])?,
                        "poll" => p.poll = pick(&[])?,
                        "fetch" => p.fetch = pick(&["ok", "bad"])?,
                        _ => return Err(format!("bad policy key {k}")),
                    }
                }
            }
            "run" => self.sim.run(&self.policy),
            "step" => {
                self.sim.commit();
                if let Some(i) = self.sim.pending.pop_front() {
                    self.sim.exec(i, &self.policy);
                }
            }
            "expect-none" if self.checked => {
                if self.id(arg(1)?).is_ok() {
                    return Err(format!("sleeve {} exists", arg(1)?));
                }
            }
            "commit" => self.sim.commit(),
            "crash" => self.sim.crash(flag("empty")),
            "job" => {
                self.sim.apply(E::CreateJob {
                    request_id: arg(1)?.into(),
                    spec: JobSpec {
                        digest: "d".into(),
                        arch: "arm64".into(),
                        command: "make test".into(),
                        even_if_present: false,
                    },
                });
            }
            "cancel-job" | "requeue-job" | "abandon-job" => {
                let job = JobId(num(arg(1)?)?);
                self.sim.apply(match words[0] {
                    "cancel-job" => E::CancelJob { job },
                    "requeue-job" => E::RequeueJob { job },
                    _ => E::AbandonJob { job },
                });
            }
            "expect" if self.checked => self.expect(&words)?,
            "expect-effect" if self.checked => {
                let key = arg(1)?;
                let all: Vec<&Effect> = self
                    .sim
                    .st
                    .effects
                    .values()
                    .filter(|e| e.key == key)
                    .collect();
                if all.is_empty() && words.contains(&"count=0") {
                    return Ok(());
                }
                let e = all.last().ok_or(format!("no effect {key}"))?;
                for w in &words[2..] {
                    let (k, v) = w.split_once('=').ok_or(format!("bad expectation {w}"))?;
                    let got = match k {
                        "state" => variant(&e.state),
                        "attempt" => e.attempt.to_string(),
                        "count" => all.len().to_string(),
                        _ => return Err(format!("bad key {k}")),
                    };
                    if got != v {
                        return Err(format!("effect {key}: {k} is {got}, expected {v}"));
                    }
                }
            }
            "expect-job" if self.checked => {
                let j = &self.sim.st.jobs[&JobId(num(arg(1)?)?)];
                let want = kv("state").ok_or("missing state")?;
                if variant(&j.state) != want {
                    return Err(format!("job state is {:?}, expected {want}", j.state));
                }
            }
            "expect" | "expect-effect" | "expect-job" | "expect-none" => {}
            w => return Err(format!("unknown command {w}")),
        }
        Ok(())
    }

    fn expect(&self, words: &[&str]) -> Result<(), String> {
        let name = words.get(1).ok_or("missing sleeve")?;
        let s = &self.sim.st.sleeves[&self.id(name)?];
        for w in &words[2..] {
            let (k, v) = w.split_once('=').ok_or(format!("bad expectation {w}"))?;
            let got = match k {
                "life" => variant(&s.life),
                "g" => s.g.0.to_string(),
                "activity" => variant(&s.activity),
                "guard" => variant(&s.guard.state),
                "harness" => format!("{:?}", s.target.harness).to_lowercase(),
                "queue" => s.queue.len().to_string(),
                _ => match k.strip_prefix("msg:") {
                    Some(id) => s
                        .messages
                        .get(id)
                        .map(|m| variant(&m.state))
                        .unwrap_or_default(),
                    None => return Err(format!("bad key {k}")),
                },
            };
            if got != v {
                return Err(format!("{name}: {k} is {got}, expected {v}"));
            }
        }
        Ok(())
    }
}
