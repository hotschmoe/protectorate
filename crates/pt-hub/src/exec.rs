//! Executors: one serial thread per sleeve, plus node jobs' (`jobs.rs`). Each re-checks its intent
//! against the latest published view right before acting, reports `Dispatched` before the
//! write, and feeds the outcome back to the core as an event.
//!
//! A live incarnation's pump is the one ordered stream for its sleeve: the executor hands it
//! its reports, and the pump submits each only after every harness event it has received, so
//! output read before a write never reaches the core after the write's `Dispatched`. A prompt
//! is reported by the pump itself: `Dispatched` when the driver's entry for the write comes
//! through (output read before it is the previous turn's) and the verdict when its ack does.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use pt_core::{
    Ask, Choice, EffectId, EffectKind, EffectState, EventKind, ExitCause, Gen, Harness, Intent,
    Layout, Life, Obs, ProbeResult, Proc, Sleeve, SleeveId, Target, TurnEnd,
};
use pt_drivers::{Ack, AckKind, Decision, Entry, ErrorClass, Session, Stop};
use pt_json::obj;

use crate::brief;
use crate::git;
use crate::home::now_ms;
use crate::hub::{Hub, View};
use crate::log::{debug, error, info, warning};
use crate::transcript::Note;
use crate::wire;

const ACK_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(10);
/// How long a pump waits on harness output before it looks for an executor report.
const ORDER_POLL: Duration = Duration::from_millis(10);

/// How long a confirmed prompt with no native turn id waits for the harness's turn start.
const NATIVE_WAIT: Duration = Duration::from_secs(2);

/// What the executor hands the pump, which acknowledges each once it is in.
enum Ordered {
    /// A report the pump submits in its stream.
    Report(Box<EventKind>),
    /// A prompt about to be written, which the pump reports from the driver's entries.
    Expect {
        request_id: String,
        effect: EffectId,
        attempt: u32,
    },
}

type Order = (Ordered, Sender<()>);

pub struct Executors {
    hub: Arc<Hub>,
    sleeves: HashMap<SleeveId, Sender<Intent>>,
    jobs: crate::jobs::JobExec,
}

fn intent_sleeve(i: &Intent) -> Option<SleeveId> {
    match i {
        Intent::Execute { kind, .. } | Intent::Probe { kind, .. } => kind.sleeve(),
        Intent::ProbeScope { sleeve, .. }
        | Intent::SelectTarget { sleeve, .. }
        | Intent::Checkpoint { sleeve, .. } => Some(*sleeve),
        _ => None,
    }
}

impl Executors {
    pub fn new(hub: Arc<Hub>) -> Executors {
        Executors {
            jobs: crate::jobs::JobExec::new(hub.clone()),
            hub,
            sleeves: HashMap::new(),
        }
    }

    /// Hands an intent to its executor. Replies and notices never come here.
    pub fn route(&mut self, intent: Intent) {
        let Some(id) = intent_sleeve(&intent) else {
            return self.jobs.route(intent);
        };
        if let Some(tx) = self.sleeves.get(&id) {
            let _ = tx.send(intent);
            return;
        }
        let view = self.hub.view();
        if view
            .state
            .sleeve(id)
            .is_none_or(|s| s.life == Life::Removed)
        {
            return refuse(&self.hub, intent, "the sleeve is removed");
        }
        let (tx, rx) = mpsc::channel();
        let exec = SleeveExec {
            hub: self.hub.clone(),
            id,
            live: None,
        };
        match thread::Builder::new()
            .name(format!("pt-exec-{}", id.0))
            .spawn(move || exec.run(rx))
        {
            Ok(_) => {
                let _ = tx.send(intent);
                self.sleeves.insert(id, tx);
            }
            Err(e) => {
                error!("sleeve {}: spawn executor: {e}", id.0);
                refuse(&self.hub, intent, &format!("spawn executor: {e}"));
            }
        }
    }

    /// Lets a removed sleeve's executor finish what it was handed and end; later intents for it
    /// are answered here.
    pub fn release(&mut self, id: SleeveId) {
        if self.sleeves.remove(&id).is_none() {
            self.cleanup_removed([id]);
        }
    }

    pub fn cleanup_removed(&self, ids: impl IntoIterator<Item = SleeveId>) {
        let paths: Vec<_> = ids
            .into_iter()
            .map(|id| self.hub.home.sleeve_scratch(id))
            .collect();
        if !paths.is_empty()
            && let Err(e) = spawn("pt-clean-scratch".into(), move || {
                for path in paths {
                    cleanup_scratch(&path);
                }
            })
        {
            warning!("{e}");
        }
    }
}

fn cleanup_scratch(path: &Path) {
    if let Err(e) = fs::remove_dir_all(path)
        && e.kind() != io::ErrorKind::NotFound
    {
        warning!("remove sleeve scratch {}: {e}", path.display());
    }
}

/// Answers an intent no executor will run: an effect is refused, a probe cannot tell.
pub(crate) fn refuse(hub: &Hub, intent: Intent, why: &str) {
    match intent {
        Intent::Execute {
            effect, attempt, ..
        } => {
            hub.submit(EventKind::Dispatched { effect, attempt });
            hub.submit(EventKind::Nacked {
                effect,
                attempt,
                reason: why.to_string(),
            });
        }
        Intent::Probe {
            effect, attempt, ..
        } => hub.submit(EventKind::Probed {
            effect,
            attempt,
            result: ProbeResult::Unknowable,
        }),
        other => debug!("dropped {other:?}: {why}"),
    }
}

/// Runs `f` on a new named thread; the error says what could not start.
fn spawn(name: String, f: impl FnOnce() + Send + 'static) -> Result<(), String> {
    thread::Builder::new()
        .name(name.clone())
        .spawn(f)
        .map(drop)
        .map_err(|e| format!("spawn {name}: {e}"))
}

struct Live {
    g: Gen,
    session: Arc<Session>,
    order: Sender<Order>,
}

struct SleeveExec {
    hub: Arc<Hub>,
    id: SleeveId,
    live: Option<Live>,
}

struct Prompt<'a> {
    g: Gen,
    effect: EffectId,
    attempt: u32,
    request_id: &'a str,
    text: &'a str,
    /// The turn a steer is for; none for a prompt.
    steer: Option<&'a str>,
}

enum Outcome {
    Acked(Option<String>),
    Nacked(String),
    Lost,
    /// The write's evidence comes another way (the launch receipt).
    Reported,
}

/// The working directory an incarnation runs in: its claim, or a scratch dir for a read-only
/// view (the root is visible read-only at its own path through the sandbox's `/`).
pub fn workdir(hub: &Hub, s: &Sleeve) -> PathBuf {
    match s.layout {
        Layout::View => hub.home.sleeve_dir(s.id).join("view"),
        Layout::Root | Layout::Worktree => PathBuf::from(&s.claim.path),
    }
}

pub fn driver_harness(h: Harness) -> Option<pt_drivers::Harness> {
    Some(match h {
        Harness::Claude => pt_drivers::Harness::Claude,
        Harness::Codex => pt_drivers::Harness::Codex,
        Harness::Grok => pt_drivers::Harness::Grok,
        Harness::PiVendor | Harness::PiLocal => pt_drivers::Harness::Pi,
        Harness::Antigravity => return None,
    })
}

/// The harness executable from the config, resolved on PATH when it has no slash.
pub fn program(hub: &Hub, h: Harness) -> Option<PathBuf> {
    let c = &hub.config;
    let p = match driver_harness(h)? {
        pt_drivers::Harness::Claude => &c.claude_program,
        pt_drivers::Harness::Codex => &c.codex_program,
        pt_drivers::Harness::Grok => &c.grok_program,
        pt_drivers::Harness::Pi => &c.pi_program,
    };
    if p.contains('/') {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(p))
        .find(|p| p.is_file())
}

fn provision_barrier(hub: &Hub, id: SleeveId) -> PathBuf {
    hub.home.sleeve_dir(id).join("provisioned")
}

/// The completion barrier of a sleeve's Provision: written only after the layout is whole.
fn provisioned(hub: &Hub, id: SleeveId) -> bool {
    provision_barrier(hub, id).is_file()
}

/// The workspaces a sleeve's sandbox hides: every other live claim except its parent's (a
/// worktree child's git dirs and a view's files are there) and its own children's, and none
/// that holds its own workspace or its parent's.
fn other_claims(st: &pt_core::State, s: &Sleeve) -> Vec<PathBuf> {
    let parent = s
        .parent
        .and_then(|p| st.sleeve(p))
        .map(|p| PathBuf::from(&p.claim.path));
    let own = PathBuf::from(&s.claim.path);
    st.sleeves
        .values()
        .filter(|o| o.id != s.id && o.life != Life::Removed)
        .filter(|o| Some(o.id) != s.parent && o.parent != Some(s.id))
        .map(|o| PathBuf::from(&o.claim.path))
        .filter(|p| !own.starts_with(p) && parent.as_ref().is_none_or(|q| !q.starts_with(p)))
        .collect()
}

/// Whether the record is still exactly what this intent was released for.
pub(crate) fn fresh(view: &View, effect: EffectId, attempt: u32, kind: &EffectKind) -> bool {
    let st = &view.state;
    let Some(e) = st.effects.get(&effect) else {
        return false;
    };
    if e.attempt != attempt || e.state != EffectState::Intent {
        return false;
    }
    if let Some((sid, g)) = kind.incarnation()
        && st.sleeve(sid).is_none_or(|s| s.g != g)
    {
        return false;
    }
    if let EffectKind::StartIncarnation { sleeve, g, .. } = kind {
        return st
            .sleeve(*sleeve)
            .is_some_and(|s| s.g == *g && s.proc == Proc::Starting(effect));
    }
    true
}

/// Polls until the scope has no tasks. A scope that will not empty is killed after 30 s.
fn wait_scope_empty(scope: &str) {
    let start = Instant::now();
    let mut killed = false;
    let mut warned = Instant::now();
    loop {
        match pt_sandbox::scope_alive(scope) {
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(e) => debug!("probe {scope}: {e}"),
        }
        if !killed && start.elapsed() > Duration::from_secs(30) {
            killed = true;
            if let Err(e) = pt_sandbox::kill_scope(scope) {
                warning!("kill {scope}: {e}");
            }
        }
        if warned.elapsed() > Duration::from_secs(60) {
            warned = Instant::now();
            warning!("{scope} still has tasks after {:?}", start.elapsed());
        }
        let step = if start.elapsed() < Duration::from_secs(10) {
            100
        } else {
            1000
        };
        thread::sleep(Duration::from_millis(step));
    }
}

fn oom_kills(file: &mut fs::File) -> Result<u64, String> {
    file.rewind().map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(|e| e.to_string())?;
    text.lines()
        .find_map(|line| line.strip_prefix("oom_kill "))
        .ok_or_else(|| "memory.events has no oom_kill counter".to_string())?
        .trim()
        .parse()
        .map_err(|e| format!("invalid oom_kill counter: {e}"))
}

struct OomWatch {
    stop: Sender<()>,
    result: Receiver<Result<bool, String>>,
}

impl OomWatch {
    fn start(scope: String) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel();
        let (result, received) = mpsc::channel();
        spawn(format!("pt-oom-{scope}"), move || {
            let outcome = watch_oom(&scope, &stopped);
            let _ = result.send(outcome);
        })?;
        Ok(Self {
            stop,
            result: received,
        })
    }

    fn finish(&self) -> Result<bool, String> {
        let _ = self.stop.send(());
        self.result
            .recv_timeout(Duration::from_secs(3))
            .map_err(|e| format!("OOM watcher: {e}"))?
    }
}

fn watch_oom(scope: &str, stopped: &Receiver<()>) -> Result<bool, String> {
    let mut last_error = None;
    let mut file = loop {
        match pt_sandbox::scope_cgroup(scope) {
            Ok(Some(cg)) => match fs::File::open(cg.join("memory.events")) {
                Ok(file) => break file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("open memory.events: {e}")),
            },
            Ok(None) => {}
            Err(e) => last_error = Some(e.to_string()),
        }
        if !matches!(
            stopped.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            return Err(last_error.unwrap_or_else(|| {
                "scope disappeared before memory.events could be opened".into()
            }));
        }
    };
    loop {
        if oom_kills(&mut file)? > 0 {
            return Ok(true);
        }
        if !matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            return Ok(false);
        }
        pt_sys::poll_priority(&file, Duration::from_millis(100))
            .map_err(|e| format!("poll memory.events: {e}"))?;
    }
}

/// Kills any `systemd-run` of ours still about to register `scope`, and returns once it is
/// gone: one that an earlier hub started would register the scope after a probe found none.
fn kill_unregistered(scope: &str) {
    let uid = fs::metadata("/proc/self").map(|m| m.uid()).ok();
    let Ok(dir) = fs::read_dir("/proc") else {
        return;
    };
    let pids: Vec<u32> = dir
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .filter(|pid| registering(*pid, scope, uid))
        .collect();
    for pid in &pids {
        warning!("{scope}: killing systemd-run {pid}, which never registered it");
        if let Err(e) = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()
        {
            warning!("kill {pid}: {e}");
        }
    }
    let start = Instant::now();
    while pids.iter().any(|p| registering(*p, scope, uid)) {
        if start.elapsed() > Duration::from_secs(10) {
            return warning!("{scope}: systemd-run {pids:?} outlived SIGKILL");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Whether `pid` is a `systemd-run --unit <scope>` of `uid` that has not yet become the command.
fn registering(pid: u32, scope: &str, uid: Option<u32>) -> bool {
    let dir = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&dir).map(|m| m.uid()).ok() != uid {
        return false;
    }
    let Ok(cmdline) = fs::read(dir.join("cmdline")) else {
        return false;
    };
    let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
    args.first().is_some_and(|a| a.ends_with(b"systemd-run"))
        && args
            .windows(2)
            .any(|w| w[0] == b"--unit" && w[1] == scope.as_bytes())
}

impl SleeveExec {
    fn run(mut self, rx: Receiver<Intent>) {
        for intent in rx {
            self.handle(intent);
        }
        if self
            .hub
            .view()
            .state
            .sleeve(self.id)
            .is_some_and(|s| s.life == Life::Removed)
        {
            cleanup_scratch(&self.hub.home.sleeve_scratch(self.id));
        }
    }

    /// Submits through the live pump when there is one, and returns once it is in: the core
    /// sees the sleeve's reports and observations in the order they happened.
    fn submit(&self, kind: EventKind) {
        if let Some(live) = &self.live {
            let (tx, rx) = mpsc::channel();
            if live
                .order
                .send((Ordered::Report(Box::new(kind.clone())), tx))
                .is_ok()
                && rx.recv().is_ok()
            {
                return;
            }
        }
        self.hub.submit(kind);
    }

    /// Tells incarnation `g`'s pump of a prompt about to be written; false when it has ended.
    fn expect(&self, g: Gen, request_id: &str, effect: EffectId, attempt: u32) -> bool {
        let Some(live) = self.live.as_ref().filter(|l| l.g == g) else {
            return false;
        };
        let (tx, rx) = mpsc::channel();
        let expect = Ordered::Expect {
            request_id: request_id.to_string(),
            effect,
            attempt,
        };
        live.order.send((expect, tx)).is_ok() && rx.recv().is_ok()
    }

    fn handle(&mut self, intent: Intent) {
        let view = self.hub.view();
        match intent {
            Intent::Execute {
                effect,
                attempt,
                kind,
            } => {
                if !fresh(&view, effect, attempt, &kind) {
                    return debug!("sleeve {}: dropped stale effect {}", self.id.0, effect.0);
                }
                if !matches!(
                    kind,
                    EffectKind::SendPrompt { .. } | EffectKind::Steer { .. }
                ) {
                    self.submit(EventKind::Dispatched { effect, attempt });
                }
                let outcome = self.execute(&view, effect, attempt, kind);
                self.submit(match outcome {
                    Outcome::Acked(native) => EventKind::Acked {
                        effect,
                        attempt,
                        native,
                    },
                    Outcome::Nacked(reason) => {
                        info!(
                            "sleeve {}: effect {} refused: {reason}",
                            self.id.0, effect.0
                        );
                        EventKind::Nacked {
                            effect,
                            attempt,
                            reason,
                        }
                    }
                    Outcome::Lost => EventKind::Lost { effect, attempt },
                    Outcome::Reported => return,
                });
            }
            Intent::Probe {
                effect,
                attempt,
                kind,
            } => {
                let probing = view
                    .state
                    .effects
                    .get(&effect)
                    .is_some_and(|e| e.attempt == attempt && e.state == EffectState::Probing);
                if probing {
                    let result = self.probe(&view, &kind);
                    self.submit(EventKind::Probed {
                        effect,
                        attempt,
                        result,
                    });
                }
            }
            Intent::ProbeScope { sleeve, g } => {
                let live = self.scope_live(g);
                debug!("sleeve {}: g{} scope live: {live}", sleeve.0, g.0);
                self.submit(EventKind::ScopeProbed { sleeve, g, live });
            }
            Intent::SelectTarget {
                sleeve,
                g,
                candidates,
                exclude,
                ..
            } => {
                let hub = &self.hub;
                let eligible = |t: &Target| {
                    program(hub, t.harness).is_some()
                        && crate::doctor::check(hub, t.harness).is_ok()
                };
                let quotas = hub.quotas.list();
                // Envoy whose own harness doctor refused parks rather than moving elsewhere.
                let envoy_gated = Hub::envoy_id(&view.state) == Some(sleeve)
                    && hub.doctor.gated(sleeve).is_some();
                let chosen = if envoy_gated {
                    Err("Envoy's harness failed doctor")
                } else {
                    crate::select::select_target(
                        now_ms(),
                        &candidates,
                        &exclude,
                        &eligible,
                        &quotas,
                    )
                };
                match &chosen {
                    Ok((t, why)) => {
                        info!(
                            "sleeve {}: resleeve target {:?}: {why}",
                            sleeve.0, t.harness
                        )
                    }
                    Err(why) => info!("sleeve {}: {why}", sleeve.0),
                }
                let target = chosen.ok().map(|(t, _)| t);
                self.submit(EventKind::TargetChosen { sleeve, g, target });
            }
            Intent::Checkpoint { sleeve, g } => {
                if let Some(s) = view.state.sleeve(sleeve) {
                    self.checkpoint(s, g);
                }
            }
            other => debug!("sleeve {}: ignored {other:?}", self.id.0),
        }
    }

    fn live_for(&self, g: Gen) -> Option<Arc<Session>> {
        self.live
            .as_ref()
            .filter(|l| l.g == g)
            .map(|l| l.session.clone())
    }

    fn scope_live(&self, g: Gen) -> bool {
        if !self.hub.config.scopes {
            return self.live_for(g).is_some_and(|s| !s.is_exited());
        }
        let scope = self.hub.scope_name(self.id, g);
        if self.live_for(g).is_none() {
            kill_unregistered(&scope);
        }
        for _ in 0..3 {
            match pt_sandbox::scope_alive(&scope) {
                Ok(pids) => return pids.is_some(),
                Err(e) => {
                    warning!("probe {scope}: {e}");
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
        // Unknown counts as live: the core then kills and waits for the scope to empty.
        true
    }

    fn execute(
        &mut self,
        view: &View,
        effect: EffectId,
        attempt: u32,
        kind: EffectKind,
    ) -> Outcome {
        let Some(s) = view.state.sleeve(self.id) else {
            return Outcome::Nacked("no such sleeve".into());
        };
        match kind {
            EffectKind::Provision { layout, claim, .. } => match self.provision(view, s, layout) {
                Ok(()) => Outcome::Acked(None),
                Err(e) => Outcome::Nacked(format!("provision {}: {e}", claim.path)),
            },
            EffectKind::StartIncarnation {
                g, target, resume, ..
            } => match self.launch(view, s, g, &target, resume) {
                Ok(()) => Outcome::Reported,
                Err(e) => {
                    warning!("sleeve {}: launch g{} failed: {e}", s.name, g.0);
                    Outcome::Nacked(e)
                }
            },
            EffectKind::StopIncarnation { g, .. } => {
                let stopped = match self.live_for(g) {
                    Some(session) => spawn(format!("pt-stop-{}-g{}", self.id.0, g.0), move || {
                        session.shutdown(STOP_GRACE)
                    }),
                    None => self.kill_and_watch(g),
                };
                match stopped {
                    Ok(()) => Outcome::Acked(None),
                    Err(e) => Outcome::Nacked(e),
                }
            }
            EffectKind::KillScope { g, .. } => match self.kill_and_watch(g) {
                Ok(()) => Outcome::Acked(None),
                Err(e) => Outcome::Nacked(e),
            },
            EffectKind::Interrupt { g, .. } => match self.live_for(g) {
                Some(session) => match session.interrupt() {
                    Ok(()) => Outcome::Acked(None),
                    Err(e) => Outcome::Nacked(e.to_string()),
                },
                None => Outcome::Nacked("no live session for that generation".into()),
            },
            EffectKind::SendPrompt {
                g,
                ref request_id,
                ref text,
                ..
            } => {
                let p = Prompt {
                    g,
                    effect,
                    attempt,
                    request_id,
                    text,
                    steer: None,
                };
                self.prompt(view, p)
            }
            EffectKind::Steer {
                g,
                ref request_id,
                ref text,
                ref expected_turn,
                ..
            } => {
                let p = Prompt {
                    g,
                    effect,
                    attempt,
                    request_id,
                    text,
                    steer: Some(expected_turn),
                };
                self.prompt(view, p)
            }
            EffectKind::RespondApproval {
                g, approval, allow, ..
            } => match self.live_for(g) {
                Some(session) => {
                    let d = if allow {
                        Decision::AllowOnce
                    } else {
                        Decision::Deny
                    };
                    match session.respond_approval(&approval, d) {
                        // Written, but only Codex confirms an answer (REVISIT): Uncertain.
                        Ok(()) => Outcome::Lost,
                        Err(e) => Outcome::Nacked(e.to_string()),
                    }
                }
                None => Outcome::Nacked("no live session for that generation".into()),
            },
            EffectKind::Compact { .. } => {
                Outcome::Nacked("explicit compaction is not exposed by the drivers".into())
            }
            EffectKind::MailDeliver {
                request_id, text, ..
            } => match self.inbox(view, &request_id, &text) {
                Ok(()) => Outcome::Acked(None),
                Err(e) => Outcome::Nacked(e.to_string()),
            },
            EffectKind::ContinuationBrief {
                from_gen,
                to_gen,
                forced,
                cause,
                effects,
                roster,
                ..
            } => {
                let spec = brief::Spec {
                    sleeve: s,
                    from_gen,
                    to_gen,
                    forced,
                    cause,
                    effects: &effects,
                    roster: &roster,
                };
                match brief::write(&self.hub, view, &workdir(&self.hub, s), &spec) {
                    Ok(()) => {
                        info!("sleeve {}: wrote {} for g{}", s.name, brief::FILE, to_gen.0);
                        Outcome::Acked(None)
                    }
                    Err(e) => Outcome::Nacked(e),
                }
            }
            EffectKind::NodeSubmit { .. }
            | EffectKind::NodeCancel { .. }
            | EffectKind::NodeAckCollected { .. } => Outcome::Nacked("not a sleeve effect".into()),
        }
    }

    fn probe(&self, view: &View, kind: &EffectKind) -> ProbeResult {
        let Some(s) = view.state.sleeve(self.id) else {
            return ProbeResult::Unknowable;
        };
        let applied = |b: bool| {
            if b {
                ProbeResult::Applied
            } else {
                ProbeResult::NotApplied
            }
        };
        match kind {
            EffectKind::Provision { .. } => applied(provisioned(&self.hub, s.id)),
            EffectKind::StopIncarnation { g, .. } | EffectKind::KillScope { g, .. } => {
                applied(!self.scope_live(*g))
            }
            EffectKind::Interrupt { .. } => ProbeResult::NotApplied,
            EffectKind::MailDeliver { request_id, .. } => applied(
                fs::read_to_string(self.hub.home.sleeve_dir(s.id).join("inbox.jsonl")).is_ok_and(
                    |t| {
                        t.contains(&format!(
                            "\"request_id\":{}",
                            pt_json::Value::from(request_id.as_str())
                        ))
                    },
                ),
            ),
            EffectKind::ContinuationBrief { to_gen, .. } => {
                applied(brief::written(&workdir(&self.hub, s), s.id, *to_gen))
            }
            // Unsafe kinds cannot tell "not applied" apart (12b item 1).
            _ => ProbeResult::Unknowable,
        }
    }

    /// Lays out the workspace, then writes the completion barrier: a cut-short provision is
    /// done again (finishing a half-made worktree), and only the barrier makes it applied.
    fn provision(&self, view: &View, s: &Sleeve, layout: Layout) -> Result<(), String> {
        if provisioned(&self.hub, s.id) {
            return Ok(());
        }
        let home = self.hub.home.sleeve_home(s.id);
        fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        let path = Path::new(&s.claim.path);
        match layout {
            Layout::Root => {
                if !path.is_dir() {
                    return Err("the directory does not exist".into());
                }
            }
            Layout::Worktree => {
                let parent = s
                    .parent
                    .and_then(|p| view.state.sleeve(p))
                    .ok_or("the parent is gone")?;
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                git::worktree_add(
                    Path::new(&parent.claim.path),
                    path,
                    &format!("pt/{}", s.name),
                    &self.hub.home.sleeve_dir(s.id),
                )?;
            }
            Layout::View => {
                let scratch = workdir(&self.hub, s);
                fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
            }
        }
        let barrier = provision_barrier(&self.hub, s.id);
        crate::home::write_private(&barrier, s.claim.path.as_bytes())
            .and_then(|()| fs::File::open(self.hub.home.sleeve_dir(s.id))?.sync_all())
            .map_err(|e| e.to_string())
    }

    fn launch(
        &mut self,
        view: &View,
        s: &Sleeve,
        g: Gen,
        target: &Target,
        resume: Option<String>,
    ) -> Result<(), String> {
        let hub = self.hub.clone();
        crate::doctor::gate_launch(&hub, s.id, &s.name, target.harness)?;
        let harness =
            driver_harness(target.harness).ok_or("that harness has no headless driver")?;
        let mut program = program(&hub, target.harness)
            .ok_or_else(|| format!("{} is not installed", wire::harness_wire(target.harness)))?;
        let home = hub.home.sleeve_home(s.id);
        fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        let logins = crate::logins::prepare_all(&hub.home.logins(), &home)
            .map_err(|e| format!("prepare the harness logins: {e}"))?;
        let tools = crate::toolchain::prepare(
            &hub.owner_home,
            &home,
            &hub.home.bin(),
            &std::env::var("PATH").unwrap_or_default(),
            &hub.config.sleeve_path,
        )
        .map_err(|e| format!("prepare the toolchain environment: {e}"))?;
        let work = workdir(&hub, s);
        let scratch = hub.home.sleeve_scratch(s.id);
        for dir in [scratch.join("target"), scratch.join("tmp")] {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("create scratch {}: {e}", dir.display()))?;
        }
        if pt_sys::is_memory_filesystem(&scratch)
            .map_err(|e| format!("check scratch filesystem {}: {e}", scratch.display()))?
        {
            return Err(format!(
                "scratch {} is on a memory filesystem; use a disk-backed hub home",
                scratch.display()
            ));
        }
        let mut sb =
            pt_sandbox::SandboxSpec::new(&hub.scope_sleeve(s.id), g.0, work.clone(), home, scratch);
        sb.scope = hub.config.scopes;
        sb.masks = pt_sandbox::default_masks(&hub.owner_home);
        sb.masks.push(hub.home.root.clone());
        sb.masks.extend(other_claims(&view.state, s));
        // Of the hub's state only the sleeves' socket and the CLI, read-only, and this harness's
        // shared login: a sleeve cannot reach the owner's socket, or replace or unlink what other
        // sleeves use. The CLI link goes through `releases/current` when the hub runs from its
        // release store, so the store is bound whole and a hub update reaches running sleeves.
        sb.ro_paths.push(hub.home.sleeve_run());
        sb.ro_paths.push(hub.home.bin());
        let releases = hub.home.root.join("releases");
        if releases.is_dir() {
            sb.ro_paths.push(releases);
        } else if let Ok(cli) = fs::canonicalize(hub.home.bin().join("protectorate")) {
            sb.ro_paths.push(cli);
        }
        // A vendor may install its program under a masked dir (grok in ~/.grok/downloads), and
        // a link on the way there may be masked too, so the sleeve runs the resolved file.
        if let Ok(real) = fs::canonicalize(&program) {
            sb.ro_paths.push(real.clone());
            program = real;
        }
        sb.rw_paths.extend(logins.dirs);
        // A cache that is itself masked (another sleeve's claim) stays out, or no sleeve starts.
        let masked: Vec<PathBuf> = sb
            .masks
            .iter()
            .filter_map(|m| fs::canonicalize(m).ok())
            .collect();
        sb.rw_paths.extend(
            tools
                .rw
                .into_iter()
                .filter(|p| fs::canonicalize(p).is_ok_and(|c| !masked.contains(&c))),
        );
        if s.layout == Layout::Worktree {
            let parent = s
                .parent
                .and_then(|p| view.state.sleeve(p))
                .ok_or("the parent is gone")?;
            sb.git_dirs = pt_sandbox::resolve_git_dirs(&work, Path::new(&parent.claim.path))
                .map_err(|e| e.to_string())?;
        }
        let token = hub.tokens.sleeve(s.id);
        sb.env = vec![
            ("PROTECTORATE_TOKEN".into(), token),
            (
                "PROTECTORATE_SOCKET".into(),
                hub.home.sleeve_socket().to_string_lossy().into_owned(),
            ),
            ("PROTECTORATE_SLEEVE".into(), s.name.clone()),
            ("PATH".into(), tools.path),
        ];
        sb.env.extend(tools.env);
        sb.env.extend(logins.env);
        let prefix = sb.command().map_err(|e| e.to_string())?;
        let mut spec = pt_drivers::SpawnSpec::new(harness, program, work);
        spec.g = g.0;
        spec.model = Some(target.model.clone()).filter(|m| !m.is_empty());
        spec.provider = Some(target.provider.clone()).filter(|p| !p.is_empty());
        spec.effort = s.effort.clone();
        spec.resume = resume;
        spec.prefix = Some(prefix);
        let name = wire::harness_wire(target.harness);
        if hub.config.supervised.iter().any(|h| h == name) {
            spec.permission = pt_drivers::Permission::Supervised;
        }
        let (session, events) = pt_drivers::spawn(spec).map_err(|e| e.to_string())?;
        let scope = hub.scope_name(s.id, g);
        let oom = if hub.config.scopes {
            match OomWatch::start(scope.clone()) {
                Ok(watch) => Some(watch),
                Err(e) => {
                    warning!("{scope}: {e}");
                    None
                }
            }
        } else {
            None
        };
        info!(
            "sleeve {}: launched g{} ({}) in {scope}, pid {}",
            s.name,
            g.0,
            wire::harness_wire(target.harness),
            session.pid()
        );
        self.submit(EventKind::Launched {
            sleeve: s.id,
            g,
            scope: scope.clone(),
            pid: session.pid(),
        });
        let session = Arc::new(session);
        let (order, ordered) = mpsc::channel();
        self.live = Some(Live {
            g,
            session: session.clone(),
            order,
        });
        let run = Run {
            id: s.id,
            name: s.name.clone(),
            g,
            harness: wire::api_harness(target.harness),
            scope,
            scoped: hub.config.scopes,
            oom,
        };
        thread::Builder::new()
            .name(format!("pt-pump-{}-g{}", s.id.0, g.0))
            .spawn(move || pump(&hub, &run, events, ordered))
            .map_err(|e| format!("spawn pump: {e}"))?;
        Ok(())
    }

    /// Kills incarnation `g`. Its pump, when this hub runs one, reports `Exited` once the scope
    /// is empty, after the output it read; otherwise a watcher does.
    fn kill_and_watch(&self, g: Gen) -> Result<(), String> {
        let live = self.live_for(g);
        if let Some(session) = live.clone() {
            spawn(format!("pt-kill-{}-g{}", self.id.0, g.0), move || {
                session.shutdown(Duration::ZERO)
            })?;
        }
        if !self.hub.config.scopes {
            return Ok(());
        }
        let scope = self.hub.scope_name(self.id, g);
        pt_sandbox::kill_scope(&scope).map_err(|e| e.to_string())?;
        if live.is_some() {
            return Ok(());
        }
        let hub = self.hub.clone();
        let id = self.id;
        spawn(format!("pt-watch-{}-g{}", id.0, g.0), move || {
            wait_scope_empty(&scope);
            hub.submit(EventKind::Exited {
                sleeve: id,
                g,
                cause: ExitCause::Killed,
            });
        })
    }

    /// Writes a prompt. Its `Dispatched` and verdict come from the pump, which sees the write's
    /// entry and the ack in order with the output; only a refused write, or an ack that never
    /// came, is reported here. To an incarnation that has ended nothing is reported: the record
    /// stays `Intent`, and the core returns its message to the queue once `Exited` fences it.
    fn prompt(&self, view: &View, p: Prompt) -> Outcome {
        let ended = || {
            debug!(
                "sleeve {}: g{} has ended; prompt {} waits for the next",
                self.id.0, p.g.0, p.request_id
            );
            Outcome::Reported
        };
        let Some(session) = self.live_for(p.g) else {
            return ended();
        };
        if !self.expect(p.g, p.request_id, p.effect, p.attempt) {
            return ended();
        }
        let from = Hub::from_label(&view.state, self.id, p.request_id);
        if Hub::envoy_id(&view.state) == Some(self.id) {
            self.note_envoy_origin(&from, p.steer.is_some());
        }
        let sent = match p.steer {
            Some(turn) => session.steer(p.request_id, &from, p.text, turn),
            None => session.send_user(p.request_id, &from, p.text),
        };
        match sent {
            Ok(()) => {}
            Err(pt_drivers::DriverError::Ended) => return ended(),
            Err(e) => {
                self.submit(EventKind::Dispatched {
                    effect: p.effect,
                    attempt: p.attempt,
                });
                return Outcome::Nacked(e.to_string());
            }
        }
        match session.wait_ack(p.request_id, ACK_TIMEOUT) {
            Ack::Confirmed { .. } | Ack::Rejected { .. } => Outcome::Reported,
            Ack::Pending => Outcome::Lost,
        }
    }

    /// D9: records who started Envoy's next turn before its prompt is written. A prompt starts
    /// a new turn; a steer from an agent taints the running one.
    fn note_envoy_origin(&self, from: &str, steer: bool) {
        let agent = from.starts_with("agent:").then(|| from.to_string());
        let mut t = crate::transcript::lock(&self.hub.envoy_turn);
        if !steer {
            t.origin = agent;
            t.spawns = 0;
        } else if agent.is_some() {
            t.origin = agent;
        }
        debug!("envoy turn origin: {:?}", t.origin);
    }

    fn inbox(&self, view: &View, request_id: &str, text: &str) -> io::Result<()> {
        let path = self.hub.home.sleeve_dir(self.id).join("inbox.jsonl");
        let line = obj! {
            "request_id" => request_id,
            "from" => Hub::from_label(&view.state, self.id, request_id),
            "text" => text,
            "time_ms" => now_ms(),
        };
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        writeln!(f, "{line}")
    }

    fn checkpoint(&self, s: &Sleeve, g: Gen) {
        let work = workdir(&self.hub, s);
        if !git::is_repo(&work) {
            return;
        }
        let dir = self.hub.home.sleeve_dir(s.id);
        let msg = format!("protectorate checkpoint {} g{}", s.name, g.0);
        match git::checkpoint(&work, &dir, &msg) {
            Ok(commit) => {
                let line = obj! { "g" => g.0, "commit" => commit.as_str(), "time_ms" => now_ms() };
                let path = dir.join("checkpoints.jsonl");
                let wrote = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| writeln!(f, "{line}"));
                if let Err(e) = wrote {
                    warning!("{}: {e}", path.display());
                }
                debug!("sleeve {}: checkpoint {commit}", s.name);
            }
            Err(e) => warning!("sleeve {}: checkpoint: {e}", s.name),
        }
    }
}

fn tool_hash(name: &str, input: &Option<pt_json::Value>) -> u64 {
    let mut h = pt_hash::Sha256::new();
    h.update(name.as_bytes());
    if let Some(v) = input {
        h.update(v.to_string().as_bytes());
    }
    let d = h.finalize();
    u64::from_le_bytes(d[..8].try_into().expect("8 bytes"))
}

/// One incarnation, as its pump sees it.
struct Run {
    id: SleeveId,
    name: String,
    g: Gen,
    harness: pt_api::Harness,
    scope: String,
    scoped: bool,
    oom: Option<OomWatch>,
}

/// An answer the harness offers; its own "cancel" is not one the owner picks.
fn choice(d: Decision) -> Option<Choice> {
    match d {
        Decision::AllowOnce => Some(Choice::AllowOnce),
        Decision::AllowSession => Some(Choice::AllowSession),
        Decision::Deny => Some(Choice::Deny),
        Decision::Cancel => None,
    }
}

fn input_text(input: &Option<pt_json::Value>) -> String {
    let t = input.as_ref().map(|v| v.to_string()).unwrap_or_default();
    match t.char_indices().nth(2000) {
        Some((at, _)) => format!("{}...", &t[..at]),
        None => t,
    }
}

/// Reads one incarnation's driver events: stores them in the transcript, turns them into
/// observations for the core, and reports `Exited` once the scope is confirmed empty.
/// Records each harness event and submits its observations; between events (and every
/// `ORDER_POLL`) it submits the executor's reports after draining what it has received.
fn pump(hub: &Hub, run: &Run, events: Receiver<pt_drivers::Event>, ordered: Receiver<Order>) {
    let mut p = Pump {
        last_error: None,
        ready: false,
        refusal: None,
        prev_error: None,
        cause: ExitCause::Killed,
        signal: None,
        expects: HashMap::new(),
        held: None,
    };
    'run: loop {
        match events.recv_timeout(ORDER_POLL) {
            Ok(ev) => {
                if p.event(hub, run, ev) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if p.held
            .as_ref()
            .is_some_and(|h| h.at.elapsed() >= NATIVE_WAIT)
        {
            p.flush_held(hub, None);
        }
        while let Ok((o, done)) = ordered.try_recv() {
            while let Ok(ev) = events.try_recv() {
                if p.event(hub, run, ev) {
                    p.order(hub, o);
                    let _ = done.send(());
                    break 'run;
                }
            }
            p.order(hub, o);
            let _ = done.send(());
        }
    }
    p.flush_held(hub, None);
    // Reports still queued come after all of this incarnation's output; later ones go direct.
    while let Ok((o, done)) = ordered.try_recv() {
        p.order(hub, o);
        let _ = done.send(());
    }
    drop(ordered);
    let Run {
        ref name,
        id,
        g,
        ref scope,
        scoped,
        ..
    } = *run;
    if scoped {
        wait_scope_empty(scope);
    }
    if let Some(watch) = &run.oom {
        match watch.finish() {
            Ok(true) if p.signal == Some(i64::from(pt_sys::SIGKILL)) => p.cause = ExitCause::Oom,
            Ok(true) => {}
            Ok(false) => {}
            Err(e) => warning!("{scope}: OOM status unknown: {e}"),
        }
    }
    info!("sleeve {name}: g{} exited ({:?})", g.0, p.cause);
    hub.submit(EventKind::Exited {
        sleeve: id,
        g,
        cause: p.cause,
    });
}

struct Pump {
    last_error: Option<ErrorClass>,
    /// The session announced itself (session_info).
    ready: bool,
    /// An auth or quota error the harness gave before it was ready: a refused handshake.
    refusal: Option<ErrorClass>,
    /// The class of the entry just before this one, when that was an error: the exit error
    /// the driver writes right before `Exited`.
    prev_error: Option<ErrorClass>,
    cause: ExitCause,
    signal: Option<i64>,
    /// Prompts being written, by request id.
    expects: HashMap<String, Expect>,
    /// A confirmed prompt whose ack had no native turn id: the harness's turn start may.
    held: Option<Held>,
}

struct Expect {
    effect: EffectId,
    attempt: u32,
    dispatched: bool,
}

struct Held {
    request_id: String,
    effect: EffectId,
    attempt: u32,
    at: Instant,
}

impl Pump {
    fn order(&mut self, hub: &Hub, o: Ordered) {
        match o {
            Ordered::Report(kind) => {
                if let EventKind::Nacked { effect, .. } = &*kind {
                    self.expects.retain(|_, e| e.effect != *effect);
                }
                hub.submit(*kind);
            }
            Ordered::Expect {
                request_id,
                effect,
                attempt,
            } => {
                let e = Expect {
                    effect,
                    attempt,
                    dispatched: false,
                };
                self.expects.insert(request_id, e);
            }
        }
    }

    fn flush_held(&mut self, hub: &Hub, native: Option<String>) {
        if let Some(h) = self.held.take() {
            hub.submit(EventKind::Acked {
                effect: h.effect,
                attempt: h.attempt,
                native,
            });
        }
    }

    /// Reports a prompt from the driver's entries, before the observations of the same entry:
    /// `Dispatched` at its write, the verdict at its ack, and a native turn id that only the
    /// turn's start carries with the confirmation.
    fn prompt_reports(&mut self, hub: &Hub, entry: &Entry) {
        if let Some(h) = &self.held {
            let native = match entry {
                Entry::TurnStarted {
                    request_id: Some(r),
                    native_turn_id,
                } if *r == h.request_id => native_turn_id.clone(),
                _ => None,
            };
            self.flush_held(hub, native);
        }
        // A turn start ahead of its ack confirms the prompt and binds the turn.
        if let Entry::TurnStarted {
            request_id: Some(r),
            native_turn_id: Some(native),
        } = entry
            && self.expects.get(r).is_some_and(|e| e.dispatched)
            && let Some(e) = self.expects.remove(r)
        {
            return hub.submit(EventKind::Acked {
                effect: e.effect,
                attempt: e.attempt,
                native: Some(native.clone()),
            });
        }
        let (Entry::UserMessage { request_id, .. } | Entry::PromptAck { request_id, .. }) = entry
        else {
            return;
        };
        let Some(e) = self.expects.get_mut(request_id) else {
            return;
        };
        let (effect, attempt) = (e.effect, e.attempt);
        if !e.dispatched {
            e.dispatched = true;
            hub.submit(EventKind::Dispatched { effect, attempt });
        }
        let Entry::PromptAck {
            ack,
            native_turn_id,
            detail,
            ..
        } = entry
        else {
            return;
        };
        self.expects.remove(request_id);
        match (ack, native_turn_id) {
            (AckKind::Confirmed, None) => {
                self.held = Some(Held {
                    request_id: request_id.clone(),
                    effect,
                    attempt,
                    at: Instant::now(),
                })
            }
            (AckKind::Confirmed, native) => hub.submit(EventKind::Acked {
                effect,
                attempt,
                native: native.clone(),
            }),
            (AckKind::Rejected, _) => {
                let reason = detail.clone().unwrap_or_else(|| "rejected".into());
                info!("prompt {request_id} refused: {reason}");
                hub.submit(EventKind::Nacked {
                    effect,
                    attempt,
                    reason,
                });
            }
        }
    }

    /// Records one harness event and submits its observations; true once the harness exited.
    fn event(&mut self, hub: &Hub, run: &Run, ev: pt_drivers::Event) -> bool {
        let Run {
            id,
            ref name,
            g,
            harness,
            ..
        } = *run;
        let entry = ev.entry;
        if let Entry::AssistantText { r#final: false, .. }
        | Entry::Reasoning { r#final: false, .. } = entry
        {
            hub.delta(id, g.0, ev.time_ms, entry);
            return false;
        }
        self.prompt_reports(hub, &entry);
        let before = self.prev_error.take();
        let mut obs = Vec::new();
        match &entry {
            Entry::SessionInfo { native_id, .. } => {
                self.ready = true;
                obs.push(Obs::Ready {
                    session: native_id.clone(),
                })
            }
            Entry::ToolCall { name, input, .. } => obs.push(Obs::Progress {
                tool: Some(tool_hash(name, input)),
            }),
            Entry::ToolResult { .. }
            | Entry::AssistantText { .. }
            | Entry::Reasoning { .. }
            | Entry::FileChange { .. } => obs.push(Obs::Progress { tool: None }),
            Entry::TurnStarted { .. } => {
                self.last_error = None;
                obs.push(Obs::Progress { tool: None });
            }
            Entry::ApprovalRequest {
                id: native,
                name: tool,
                summary,
                input,
                options,
                ..
            } => obs.push(Obs::ApprovalRequested {
                id: native.clone(),
                ask: Some(Ask {
                    title: tool.clone(),
                    summary: summary.clone(),
                    detail: input_text(input),
                    options: options.iter().filter_map(|d| choice(*d)).collect(),
                }),
            }),
            Entry::ApprovalDecision { id: native, .. } => {
                obs.push(Obs::ApprovalResolved { id: native.clone() })
            }
            Entry::TurnSettled { stop, usage, .. } => {
                let tokens = usage.input + usage.output;
                if tokens > 0 {
                    obs.push(Obs::Usage { tokens });
                }
                let mut end = match stop {
                    Stop::Cancelled => TurnEnd::Interrupted,
                    Stop::Error => match self.last_error {
                        Some(ErrorClass::Quota) => TurnEnd::UsageLimit,
                        Some(ErrorClass::Auth) => TurnEnd::AuthFailed,
                        _ => TurnEnd::Error,
                    },
                    _ => TurnEnd::Done,
                };
                if hub.fault.as_ref().is_some_and(|f| f.fire(name)) {
                    warning!("sleeve {name}: PT_TEST_FAULT reports this turn as a usage limit");
                    end = TurnEnd::UsageLimit;
                }
                self.last_error = None;
                obs.push(Obs::TurnSettled { end });
            }
            Entry::Error { class, text } => {
                self.last_error = Some(*class);
                self.prev_error = Some(*class);
                if !self.ready && matches!(class, ErrorClass::Auth | ErrorClass::Quota) {
                    self.refusal.get_or_insert(*class);
                }
                if *class == ErrorClass::Protocol {
                    obs.push(Obs::Desync {
                        detail: text.clone(),
                    });
                }
            }
            Entry::Compaction { .. } => obs.push(Obs::CompactDone),
            Entry::Exited { code, signal, .. } => {
                self.signal = *signal;
                self.cause = match (self.refusal.or(before), code, signal) {
                    (Some(ErrorClass::Auth), ..) => ExitCause::Auth,
                    (Some(ErrorClass::Quota), ..) => ExitCause::Quota,
                    (_, Some(c), _) => ExitCause::Exit(*c as i32),
                    _ => ExitCause::Killed,
                };
            }
            Entry::UserMessage { .. } | Entry::PromptAck { .. } | Entry::Notice { .. } => {}
        }
        let exited = matches!(entry, Entry::Exited { .. });
        let settles = exited || matches!(entry, Entry::TurnSettled { .. });
        if settles && Hub::envoy_id(&hub.view().state) == Some(id) {
            *crate::transcript::lock(&hub.envoy_turn) = Default::default();
        }
        let note = Note {
            harness: matches!(entry, Entry::SessionInfo { .. }).then_some(harness),
            level: None,
        };
        // Stored before the core hears of it, so a settled view always has its text on disk.
        hub.record(id, g.0, ev.time_ms, entry, note);
        for o in obs {
            hub.submit(EventKind::Observed {
                sleeve: id,
                g,
                obs: o,
            });
        }
        exited
    }
}
