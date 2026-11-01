//! Executors: one serial thread per sleeve, plus one for node jobs. Each re-checks its intent
//! against the latest published view right before acting, reports `Dispatched` before the
//! write, and feeds the outcome back to the core as an event.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use pt_core::{
    EffectId, EffectKind, EffectState, EventKind, ExitCause, Gen, Harness, Intent, Layout, Obs,
    ProbeResult, Proc, Sleeve, SleeveId, Target, TurnEnd,
};
use pt_drivers::{Ack, Decision, Entry, ErrorClass, Session, Stop};
use pt_json::obj;

use crate::approvals::{ToolAsk, tool_id};
use crate::brief;
use crate::git;
use crate::home::now_ms;
use crate::hub::{Hub, View};
use crate::log::{debug, info, warning};
use crate::transcript::Note;
use crate::wire;

const ACK_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(10);

pub struct Executors {
    hub: Arc<Hub>,
    sleeves: HashMap<SleeveId, Sender<Intent>>,
    jobs: Sender<Intent>,
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
        let (jobs, rx) = mpsc::channel::<Intent>();
        thread::Builder::new()
            .name("pt-exec-jobs".into())
            .spawn(move || {
                for i in rx {
                    debug!("jobs: node execution is unavailable; holding {i:?}");
                }
            })
            .expect("spawn jobs executor");
        Executors {
            hub,
            sleeves: HashMap::new(),
            jobs,
        }
    }

    /// Hands an intent to its executor. Replies and notices never come here.
    pub fn route(&mut self, intent: Intent) {
        let Some(id) = intent_sleeve(&intent) else {
            let _ = self.jobs.send(intent);
            return;
        };
        let tx = self.sleeves.entry(id).or_insert_with(|| {
            let (tx, rx) = mpsc::channel();
            let exec = SleeveExec {
                hub: self.hub.clone(),
                id,
                live: None,
            };
            thread::Builder::new()
                .name(format!("pt-exec-{}", id.0))
                .spawn(move || exec.run(rx))
                .expect("spawn sleeve executor");
            tx
        });
        let _ = tx.send(intent);
    }
}

struct Live {
    g: Gen,
    session: Arc<Session>,
}

struct SleeveExec {
    hub: Arc<Hub>,
    id: SleeveId,
    live: Option<Live>,
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
        Harness::PiVendor | Harness::PiLocal | Harness::PiDurable => pt_drivers::Harness::Pi,
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

/// Harnesses `doctor.json` marks ineligible (`{"ineligible": ["claude"]}`).
fn doctor_ineligible(hub: &Hub, h: Harness) -> bool {
    let Ok(bytes) = fs::read(hub.home.doctor()) else {
        return false;
    };
    let Ok(v) = pt_json::parse_bytes(&bytes) else {
        return false;
    };
    let name = wire::harness_wire(h);
    v["ineligible"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(name)))
}

/// Login files copied from the owner's home into a sleeve's private home (REVISIT: a
/// doctor-guided login per sleeve home is right). Existing files are left alone, since the
/// harness may have refreshed them.
fn credential_files(h: Harness) -> &'static [&'static str] {
    match h {
        Harness::Claude => &[".claude/.credentials.json", ".claude.json"],
        Harness::Codex => &[".codex/auth.json", ".codex/config.toml"],
        Harness::Grok => &[".grok/auth.json"],
        Harness::PiVendor | Harness::PiLocal | Harness::PiDurable => &[
            ".pi/agent/auth.json",
            ".pi/agent/models.json",
            ".pi/agent/settings.json",
        ],
        Harness::Antigravity => &[],
    }
}

fn seed_credentials(owner_home: &Path, home: &Path, h: Harness) -> io::Result<()> {
    for rel in credential_files(h) {
        let (src, dst) = (owner_home.join(rel), home.join(rel));
        if dst.exists() || !src.is_file() {
            continue;
        }
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("seed {}: {e}", dst.display()));
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir).map_err(ctx)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(ctx)?;
        }
        fs::copy(&src, &dst).map_err(ctx)?;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o600)).map_err(ctx)?;
    }
    Ok(())
}

/// Whether the record is still exactly what this intent was released for.
fn fresh(view: &View, effect: EffectId, attempt: u32, kind: &EffectKind) -> bool {
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

impl SleeveExec {
    fn run(mut self, rx: Receiver<Intent>) {
        for intent in rx {
            self.handle(intent);
        }
    }

    fn submit(&self, kind: EventKind) {
        self.hub.submit(kind);
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
                self.submit(EventKind::Dispatched { effect, attempt });
                let outcome = self.execute(&view, kind);
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
                    program(hub, t.harness).is_some() && !doctor_ineligible(hub, t.harness)
                };
                let target = crate::select::select_target(&candidates, &exclude, &eligible);
                info!(
                    "sleeve {}: resleeve target {:?}",
                    sleeve.0,
                    target.as_ref().map(|t| t.harness)
                );
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

    fn execute(&mut self, view: &View, kind: EffectKind) -> Outcome {
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
            } => match self.launch(s, g, &target, resume) {
                Ok(()) => Outcome::Reported,
                Err(e) => {
                    warning!("sleeve {}: launch g{} failed: {e}", s.name, g.0);
                    Outcome::Nacked(e)
                }
            },
            EffectKind::StopIncarnation { g, .. } => {
                match self.live_for(g) {
                    Some(session) => {
                        thread::spawn(move || session.shutdown(STOP_GRACE));
                    }
                    None => {
                        if let Err(e) = self.kill_and_watch(g) {
                            return Outcome::Nacked(e);
                        }
                    }
                }
                Outcome::Acked(None)
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
                request_id,
                text,
                ..
            } => self.prompt(view, g, &request_id, &text, false),
            EffectKind::Steer {
                g,
                request_id,
                text,
                ..
            } => self.prompt(view, g, &request_id, &text, true),
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
            EffectKind::Provision { .. } => {
                applied(workdir(&self.hub, s).is_dir() && self.hub.home.sleeve_home(s.id).is_dir())
            }
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

    fn provision(&self, view: &View, s: &Sleeve, layout: Layout) -> Result<(), String> {
        let home = self.hub.home.sleeve_home(s.id);
        fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        let mut harnesses = vec![s.target.harness];
        harnesses.extend(s.resleeve_to.iter().map(|t| t.harness));
        for h in harnesses {
            seed_credentials(&self.hub.owner_home, &home, h).map_err(|e| e.to_string())?;
        }
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
                )?;
            }
            Layout::View => {
                let scratch = workdir(&self.hub, s);
                fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
            }
        }
        Ok(())
    }

    fn launch(
        &mut self,
        s: &Sleeve,
        g: Gen,
        target: &Target,
        resume: Option<String>,
    ) -> Result<(), String> {
        let hub = self.hub.clone();
        let harness =
            driver_harness(target.harness).ok_or("that harness has no headless driver")?;
        let program = program(&hub, target.harness)
            .ok_or_else(|| format!("{} is not installed", wire::harness_wire(target.harness)))?;
        let home = hub.home.sleeve_home(s.id);
        fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        seed_credentials(&hub.owner_home, &home, target.harness).map_err(|e| e.to_string())?;
        let work = workdir(&hub, s);
        let mut sb = pt_sandbox::SandboxSpec::new(&hub.scope_sleeve(s.id), g.0, work.clone(), home);
        sb.scope = hub.config.scopes;
        sb.masks = pt_sandbox::default_masks(&hub.owner_home);
        sb.masks.push(hub.home.root.clone());
        sb.rw_paths.push(hub.home.run());
        if s.layout == Layout::Worktree {
            sb.git_dirs = pt_sandbox::resolve_git_dirs(&work).map_err(|e| e.to_string())?;
        }
        let token = hub.tokens.sleeve(s.id);
        let path = std::env::var("PATH").unwrap_or_default();
        sb.env = vec![
            ("PROTECTORATE_TOKEN".into(), token),
            (
                "PROTECTORATE_SOCKET".into(),
                hub.home.socket().to_string_lossy().into_owned(),
            ),
            ("PROTECTORATE_SLEEVE".into(), s.name.clone()),
            (
                "PATH".into(),
                format!("{}:{path}", hub.home.bin().display()),
            ),
        ];
        let prefix = sb.command().map_err(|e| e.to_string())?;
        let mut spec = pt_drivers::SpawnSpec::new(harness, program, work);
        spec.g = g.0;
        spec.model = Some(target.model.clone()).filter(|m| !m.is_empty());
        spec.provider = Some(target.provider.clone()).filter(|p| !p.is_empty());
        spec.resume = resume;
        spec.prefix = Some(prefix);
        let (session, events) = pt_drivers::spawn(spec).map_err(|e| e.to_string())?;
        let scope = hub.scope_name(s.id, g);
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
        self.live = Some(Live {
            g,
            session: session.clone(),
        });
        let run = Run {
            id: s.id,
            name: s.name.clone(),
            g,
            harness: wire::api_harness(target.harness),
            scope,
            scoped: hub.config.scopes,
        };
        thread::Builder::new()
            .name(format!("pt-pump-{}-g{}", s.id.0, g.0))
            .spawn(move || pump(&hub, &run, events))
            .map_err(|e| format!("spawn pump: {e}"))?;
        Ok(())
    }

    fn kill_and_watch(&self, g: Gen) -> Result<(), String> {
        if let Some(session) = self.live_for(g) {
            thread::spawn(move || session.shutdown(Duration::ZERO));
        }
        if !self.hub.config.scopes {
            return Ok(());
        }
        let scope = self.hub.scope_name(self.id, g);
        pt_sandbox::kill_scope(&scope).map_err(|e| e.to_string())?;
        let hub = self.hub.clone();
        let id = self.id;
        thread::spawn(move || {
            wait_scope_empty(&scope);
            hub.submit(EventKind::Exited {
                sleeve: id,
                g,
                cause: ExitCause::Killed,
            });
        });
        Ok(())
    }

    fn prompt(&self, view: &View, g: Gen, request_id: &str, text: &str, steer: bool) -> Outcome {
        let Some(session) = self.live_for(g) else {
            return Outcome::Nacked("no live session for that generation".into());
        };
        let from = Hub::from_label(&view.state, self.id, request_id);
        if Hub::envoy_id(&view.state) == Some(self.id) {
            self.note_envoy_origin(&from, steer);
        }
        let sent = if steer {
            session.steer(request_id, &from, text)
        } else {
            session.send_user(request_id, &from, text)
        };
        if let Err(e) = sent {
            return Outcome::Nacked(e.to_string());
        }
        match session.wait_ack(request_id, ACK_TIMEOUT) {
            Ack::Confirmed { native_turn_id } => Outcome::Acked(native_turn_id),
            Ack::Rejected { detail } => {
                Outcome::Nacked(detail.unwrap_or_else(|| "rejected".into()))
            }
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
fn pump(hub: &Hub, run: &Run, events: Receiver<pt_drivers::Event>) {
    let Run {
        id,
        ref name,
        g,
        harness,
        ref scope,
        scoped,
    } = *run;
    let mut last_error: Option<ErrorClass> = None;
    let mut cause = ExitCause::Killed;
    for ev in events {
        let entry = ev.entry;
        if let Entry::AssistantText { r#final: false, .. }
        | Entry::Reasoning { r#final: false, .. } = entry
        {
            hub.delta(id, g.0, ev.time_ms, entry);
            continue;
        }
        let mut obs = Vec::new();
        match &entry {
            Entry::SessionInfo { native_id, .. } => obs.push(Obs::Ready {
                session: native_id.clone(),
            }),
            Entry::ToolCall { name, input, .. } => obs.push(Obs::Progress {
                tool: Some(tool_hash(name, input)),
            }),
            Entry::ToolResult { .. }
            | Entry::AssistantText { .. }
            | Entry::Reasoning { .. }
            | Entry::FileChange { .. } => obs.push(Obs::Progress { tool: None }),
            Entry::TurnStarted { .. } => {
                last_error = None;
                obs.push(Obs::Progress { tool: None });
            }
            Entry::ApprovalRequest {
                id: native,
                name: tool,
                summary,
                input,
                options,
                ..
            } => {
                hub.approvals.open_tool(
                    &hub.bus,
                    ToolAsk {
                        sleeve: id,
                        g: g.0,
                        native: native.clone(),
                        title: tool.clone(),
                        summary: summary.clone(),
                        detail: input_text(input),
                        options: options.iter().map(|d| wire::decision(*d)).collect(),
                    },
                );
                obs.push(Obs::ApprovalRequested { id: native.clone() })
            }
            Entry::ApprovalDecision {
                id: native,
                decision,
                by,
                ..
            } => {
                let d = wire::decision(*decision);
                let state = if d == pt_api::Decision::Deny {
                    pt_api::ApprovalState::Denied
                } else {
                    pt_api::ApprovalState::Approved
                };
                hub.approvals.resolve(
                    &hub.bus,
                    &tool_id(id, native),
                    state,
                    Some(d),
                    wire::decided_by(by.as_deref()),
                );
                obs.push(Obs::ApprovalResolved { id: native.clone() })
            }
            Entry::TurnSettled { stop, usage, .. } => {
                let tokens = usage.input + usage.output;
                if tokens > 0 {
                    obs.push(Obs::Usage { tokens });
                }
                let mut end = match stop {
                    Stop::Cancelled => TurnEnd::Interrupted,
                    Stop::Error => match last_error {
                        Some(ErrorClass::Quota | ErrorClass::RateLimit) => TurnEnd::UsageLimit,
                        Some(ErrorClass::Auth) => TurnEnd::AuthFailed,
                        _ => TurnEnd::Error,
                    },
                    _ => TurnEnd::Done,
                };
                if hub.fault.as_ref().is_some_and(|f| f.fire(name)) {
                    warning!("sleeve {name}: PT_TEST_FAULT reports this turn as a usage limit");
                    end = TurnEnd::UsageLimit;
                }
                last_error = None;
                obs.push(Obs::TurnSettled { end });
            }
            Entry::Error { class, text } => {
                last_error = Some(*class);
                if *class == ErrorClass::Protocol {
                    obs.push(Obs::Desync {
                        detail: text.clone(),
                    });
                }
            }
            Entry::Compaction { .. } => obs.push(Obs::CompactDone),
            Entry::Exited { code, signal, .. } => {
                cause = match (code, signal) {
                    (Some(c), _) => ExitCause::Exit(*c as i32),
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
        if exited {
            break;
        }
    }
    if scoped {
        wait_scope_empty(scope);
    }
    info!("sleeve {name}: g{} exited ({cause:?})", g.0);
    hub.submit(EventKind::Exited {
        sleeve: id,
        g,
        cause,
    });
}
