//! Scenario scripts: the committed seeds of `sleeve_core`.
//!
//! A script starts with `#scenario <hash>`, where `<hash>` is the FNV-1a 64 of every byte after
//! that first line, in lowercase hex. Its `expect` lines are checked only when the hash matches,
//! so libFuzzer's mutations of a seed still run as event sequences but never trip a stale
//! expectation. Every line drives the same simulated hub as the byte-driven mode, so the
//! invariants and the replay check apply to scenarios too.
//!
//! ```text
//! spawn NAME PATH [parent=NAME] [view] [harness=H] [pane] [budget=N] [disk=N] [effort=E] [to=H,H] [d9]
//! send NAME ID [steer] [from=NAME|from=agent:NAME] [d9]
//! continue NAME                    a second journaled continuation; the first is sent by the core
//! stop|remove|interrupt|compact|viewed NAME
//! start|resleeve NAME [harness=H] [effort=E]
//! approve NAME ID allow|deny [g=N]  ID: the harness's id, or action:N   abandon KEY
//! ask NAME [timeout=DURATION]       a code rule asks the owner (D9)
//! obs NAME WHAT [g=N] [noask]       WHAT: ready progress[=H] approval=ID resolved=ID
//!                                         settled[=usage|auth|interrupted|error] compactdone
//!                                         desync screen=V usage=N disk=N forbidden
//! exit NAME [oom|auth|quota] [code=N]  the incarnation's process tree dies
//! tick DURATION                     e.g. 30s 16m 2h
//! clock MILLIS                      the hub clock jumps to MILLIS (it never runs backwards)
//! policy KEY=VALUE...               see `Policy`
//! run | step [N] | commit         step N executes the Nth released intent (from 0)
//! crash [empty] [down=DURATION]     the hub dies at the last commit and restarts DURATION later
//! job ID | cancel-job N | requeue-job N | abandon-job N
//! expect NAME life=.. g=N activity=.. guard=.. harness=.. queue=N effort=.. agent=NAME|none d9=true|false msg:ID=.. from:ID=Party [park=REST]
//!   `park=` is the rest of the line, the hub's park string (`flapping (claude login rejected)`).
//! expect-reply TEXT...             Refused matches its text; any other reply matches its variant
//! expect-effect KEY state=.. attempt=N count=N
//! expect-job N state=..           expect-none NAME
//! expect-approval ID [g=N] state=.. decision=allow|deny|none orphan=.. response=KEY
//! ```
//!
//! An effect KEY ending in `*` matches the newest record whose key has that prefix.

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
        _ => return Err(format!("unknown harness {s}")),
    })
}

/// The wire name `park_wire` uses. Kept here so a scenario can check the string.
fn harness_name(h: Harness) -> &'static str {
    match h {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
        Harness::Grok => "grok",
        Harness::Antigravity => "antigravity",
        Harness::PiVendor => "pi_vendor",
        Harness::PiLocal => "pi_local",
    }
}

/// The park string the hub shows. Empty when the sleeve is not parked.
fn park_text(life: &Life) -> String {
    let Life::Parked(reason) = life else {
        return String::new();
    };
    match reason {
        ParkReason::Provision => "provision".into(),
        ParkReason::NoTarget => "no_target".into(),
        ParkReason::Launch => "launch".into(),
        ParkReason::Flapping(Some(h)) => {
            format!("flapping ({} login rejected)", harness_name(*h))
        }
        ParkReason::Flapping(None) => "flapping".into(),
        ParkReason::BriefUnconfirmed => "brief_unconfirmed".into(),
        ParkReason::Guard(g) => format!("g{}", g.number()),
        ParkReason::Ineligible { harness } => format!("ineligible ({})", harness_name(*harness)),
    }
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
    let unit = match unit {
        "s" => SECOND,
        "m" => MINUTE,
        "h" => HOUR,
        _ => return Err(format!("bad duration {s}")),
    };
    n.checked_mul(unit).ok_or(format!("duration {s} overflows"))
}

/// The variant name of a Debug-printed value: `Stopping { remove: false }` is `Stopping`.
fn variant(v: &impl std::fmt::Debug) -> String {
    let s = format!("{v:?}");
    s.split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("")
        .to_string()
}

/// A script KEY ending in `*` matches by prefix.
fn key_matches(pattern: &str, key: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => key.starts_with(prefix),
        None => key == pattern,
    }
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
            .filter(|e| key_matches(key, &e.key))
            .map(|e| e.id)
            .next_back()
            .ok_or_else(|| format!("no effect {key}"))
    }

    fn sleeve(&self, id: SleeveId) -> Result<&Sleeve, String> {
        self.sim
            .st
            .sleeves
            .get(&id)
            .ok_or_else(|| format!("no sleeve {}", id.0))
    }

    fn job(&self, id: JobId) -> Result<&Job, String> {
        self.sim
            .st
            .jobs
            .get(&id)
            .ok_or_else(|| format!("no job {}", id.0))
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
                    effort: kv("effort").map(str::to_string),
                    d9: flag("d9"),
                };
                self.sim.apply(E::Spawn(spec));
            }
            "send" => {
                let to = self.id(arg(1)?)?;
                let from = match kv("from") {
                    Some(n) if n.starts_with("agent:") => Party::Agent,
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
                    d9: flag("d9"),
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
                    "start" => E::Start {
                        sleeve,
                        target: t,
                        effort: kv("effort").map(str::to_string),
                    },
                    _ => E::Resleeve {
                        sleeve,
                        target: t,
                        effort: kv("effort").map(str::to_string),
                    },
                });
            }
            "approve" => {
                let sleeve = self.id(arg(1)?)?;
                let approval = self.approval(sleeve, arg(2)?, kv("g"))?;
                self.sim.apply(E::RespondApproval {
                    sleeve,
                    approval,
                    allow: arg(3)? == "allow",
                    note: String::new(),
                });
            }
            "ask" => {
                let sleeve = self.id(arg(1)?)?;
                let timeout = kv("timeout")
                    .map(duration)
                    .transpose()?
                    .unwrap_or(15 * MINUTE);
                self.sim.apply(E::RequestApproval {
                    sleeve,
                    ask: Ask {
                        title: "remove sleeve victim".into(),
                        summary: "rule asks".into(),
                        detail: String::new(),
                        options: vec![Choice::AllowOnce, Choice::Deny],
                    },
                    timeout,
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
                    None => self.sleeve(sleeve)?.g,
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
                    "approval" => Obs::ApprovalRequested {
                        id: v.into(),
                        ask: (!flag("noask")).then(|| Ask {
                            title: "Bash".into(),
                            summary: format!("tool call {v}"),
                            detail: String::new(),
                            options: vec![Choice::AllowOnce, Choice::AllowSession, Choice::Deny],
                        }),
                    },
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
                let g = self.sleeve(sleeve)?.g;
                let cause = if flag("oom") {
                    ExitCause::Oom
                } else if flag("auth") {
                    ExitCause::Auth
                } else if flag("quota") {
                    ExitCause::Quota
                } else {
                    ExitCause::Exit(kv("code").map(num).transpose()?.unwrap_or(1) as i32)
                };
                self.sim.crash_process(sleeve, g, cause);
            }
            "tick" => self.sim.tick(duration(arg(1)?)?),
            "clock" => {
                self.sim.now = num(arg(1)?)?;
                self.sim.apply(E::Tick);
            }
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
                        "prompt" => {
                            p.prompt = pick(&["ack", "nack", "lost", "none", "ack-no-turn"])?
                        }
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
            "continue" => {
                let sleeve = self.id(arg(1)?)?;
                self.sim.apply(E::Continue {
                    sleeve,
                    text: "The previous turn was interrupted. Continue from where it stopped."
                        .into(),
                });
            }
            "run" => self.sim.run(&self.policy),
            "step" => {
                self.sim.commit();
                let n = words.get(1).map(|n| num(n)).transpose()?.unwrap_or(0);
                self.sim.exec_at(n as usize, &self.policy);
            }
            "expect-none" if self.checked => {
                if self.id(arg(1)?).is_ok() {
                    return Err(format!("sleeve {} exists", arg(1)?));
                }
            }
            "commit" => self.sim.commit(),
            "crash" => {
                let down = kv("down").map(duration).transpose()?.unwrap_or(0);
                self.sim.now = self.sim.now.saturating_add(down);
                self.sim.crash(flag("empty"));
            }
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
            "job-exit" => {
                let job = JobId(num(arg(1)?)?);
                let (attempt, node) = {
                    let j = self.job(job)?;
                    (j.attempt, j.node.ok_or("job has no node")?)
                };
                self.sim.apply(E::JobStatus {
                    job,
                    attempt,
                    node,
                    report: JobReport::Exited { code: 0 },
                });
            }
            "artifact-result" => {
                let job = JobId(num(arg(1)?)?);
                let attempt = self.job(job)?.attempt;
                let refusals = kv("refused")
                    .map(|p| {
                        vec![ArtifactRefusal {
                            path: p.into(),
                            reason: "not found".into(),
                        }]
                    })
                    .unwrap_or_default();
                self.sim.apply(E::ArtifactCollection {
                    job,
                    attempt,
                    refusals,
                    incomplete: flag("incomplete").then(|| "fetch failed".into()),
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
            "expect" if self.checked => {
                // `park=` keeps its spaces, so it is the rest of the line and comes last.
                let (head, park) = match line.split_once(" park=") {
                    Some((head, park)) => (head, Some(park.trim())),
                    None => (line, None),
                };
                let words: Vec<&str> = head.split_whitespace().collect();
                self.expect(&words)?;
                if let Some(want) = park {
                    let name = words.get(1).copied().ok_or("missing sleeve")?;
                    let s = self.sleeve(self.id(name)?)?;
                    let got = park_text(&s.life);
                    if got != want {
                        return Err(format!("{name}: park is {got}, expected {want}"));
                    }
                }
            }
            "expect-notice" if self.checked => {
                let want = num(kv("count").ok_or("missing count")?)? as usize;
                let got = self
                    .sim
                    .notices
                    .iter()
                    .filter(|n| variant(n) == arg(1).unwrap_or(""))
                    .count();
                if got != want {
                    return Err(format!("notice {} count {got}, expected {want}", arg(1)?));
                }
            }
            "expect-reply" if self.checked => {
                let want = words[1..].join(" ");
                let got = self
                    .sim
                    .last
                    .iter()
                    .find_map(|i| match i {
                        Intent::Reply(Reply::Refused(_, why)) if why == &want => Some(why.clone()),
                        Intent::Reply(r) => Some(variant(r)),
                        _ => None,
                    })
                    .unwrap_or_default();
                if got != want {
                    return Err(format!("reply is {got:?}, expected {want:?}"));
                }
            }
            "expect-effect" if self.checked => {
                let key = arg(1)?;
                let all: Vec<&Effect> = self
                    .sim
                    .st
                    .effects
                    .values()
                    .filter(|e| key_matches(key, &e.key))
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
                let j = self.job(JobId(num(arg(1)?)?))?;
                let want = kv("state").ok_or("missing state")?;
                if variant(&j.state) != want {
                    return Err(format!("job state is {:?}, expected {want}", j.state));
                }
            }
            "expect-approval" if self.checked => self.expect_approval(&words)?,
            "expect" | "expect-effect" | "expect-job" | "expect-none" | "expect-approval"
            | "expect-notice" | "expect-reply" => {}
            w => return Err(format!("unknown command {w}")),
        }
        Ok(())
    }

    /// A script approval ID: `action:N` as is, else sleeve's tool approval at g (default the
    /// current generation).
    fn approval(&self, sleeve: SleeveId, id: &str, g: Option<&str>) -> Result<String, String> {
        if id.starts_with(ACTION_PREFIX) {
            return Ok(id.into());
        }
        let g = match g {
            Some(n) => Gen(num(n)?),
            None => self.sleeve(sleeve)?.g,
        };
        Ok(tool_approval_id(sleeve, g, id))
    }

    fn expect_approval(&self, words: &[&str]) -> Result<(), String> {
        let id = *words.get(1).ok_or("missing approval")?;
        let a = self
            .sim
            .st
            .approvals
            .values()
            .find(|a| a.id == id)
            .ok_or(format!("no approval {id}"))?;
        for w in &words[2..] {
            let (k, v) = w.split_once('=').ok_or(format!("bad expectation {w}"))?;
            let got = match k {
                "state" => variant(&a.state),
                "decision" => match a.decision {
                    Some(d) if d.allow => "allow".into(),
                    Some(_) => "deny".into(),
                    None => "none".into(),
                },
                "by" => a.decision.map(|d| variant(&d.by)).unwrap_or_default(),
                "orphan" => a.orphan.map(|o| variant(&o)).unwrap_or("none".into()),
                "response" => match a.decision.and_then(|d| d.response) {
                    Some(e) => variant(
                        &self
                            .sim
                            .st
                            .effects
                            .get(&e)
                            .ok_or(format!("no effect {}", e.0))?
                            .state,
                    ),
                    None => "none".into(),
                },
                _ => return Err(format!("bad key {k}")),
            };
            if got != v {
                return Err(format!("approval {id}: {k} is {got}, expected {v}"));
            }
        }
        Ok(())
    }

    fn expect(&self, words: &[&str]) -> Result<(), String> {
        let name = words.get(1).ok_or("missing sleeve")?;
        let s = self.sleeve(self.id(name)?)?;
        for w in &words[2..] {
            let (k, v) = w.split_once('=').ok_or(format!("bad expectation {w}"))?;
            let got = match k {
                "life" => variant(&s.life),
                "g" => s.g.0.to_string(),
                "activity" => variant(&s.activity),
                "guard" => variant(&s.guard.state),
                "harness" => format!("{:?}", s.target.harness).to_lowercase(),
                "queue" => s.queue.len().to_string(),
                "effort" => s.effort.clone().unwrap_or("none".into()),
                "agent" => s.turn_agent.clone().unwrap_or_else(|| "none".into()),
                "d9" => s.d9.to_string(),
                _ => match k.strip_prefix("msg:") {
                    Some(id) => s
                        .messages
                        .get(id)
                        .map(|m| variant(&m.state))
                        .unwrap_or_default(),
                    None => match k.strip_prefix("from:") {
                        Some(id) => s
                            .messages
                            .get(id)
                            .map(|m| variant(&m.from))
                            .ok_or_else(|| format!("no message {id}"))?,
                        None => return Err(format!("bad key {k}")),
                    },
                },
            };
            if got != v {
                return Err(format!("{name}: {k} is {got}, expected {v}"));
            }
        }
        Ok(())
    }
}
