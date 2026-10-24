//! The chat, report, orchestrator, api and audit commands against a real hub
//! running sandboxed chat sleeves over Pi RPC and Claude stream JSON peers.
//! This test binary doubles as the sandbox helper the hub launches sleeves
//! through, so it has its own main.

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
fn main() {
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg.starts_with("--internal-protectorate-sandbox"))
    {
        match protectorate_sleeve_runner::helper_main() {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("sandbox helper: {error}");
                std::process::exit(125)
            }
        }
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(linux::chat_commands_drive_real_sleeves());
    println!("test chat_commands_drive_real_sleeves ... ok");
}

#[cfg(target_os = "linux")]
mod linux {
    use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
    use serde_json::{json, Value};
    use std::{
        collections::BTreeMap,
        fs,
        io::{BufRead, BufReader},
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::mpsc,
        time::{Duration, Instant},
    };
    use tokio::net::TcpListener;

    const OWNER: &str = "owner-secret";
    const DEADLINE: Duration = Duration::from_secs(90);

    const PI: &str = r#"#!/usr/bin/python3
import json,sys
def emit(o): print(json.dumps(o),flush=True)
compacts=0
for line in sys.stdin:
 m=json.loads(line); k=m.get('type')
 def ok(data=None):
  r={'type':'response','id':m['id'],'command':k,'success':True}
  if data is not None: r['data']=data
  emit(r)
 if k=='get_state':
  ok({'sessionId':'pi-cli-session','model':{'id':'test-model','provider':'test-local','contextWindow':8192},'thinkingLevel':'medium'})
 elif k=='get_available_models':
  ok({'models':[{'id':'test-model','provider':'test-local','name':'Test Model','reasoning':True},{'id':'test-alt','provider':'test-local','name':'Alternate','reasoning':True}]})
 elif k=='get_available_thinking_levels':
  ok({'levels':['off','low','medium','high']})
 elif k=='set_model':
  ok({'id':m['modelId'],'provider':m['provider']})
 elif k=='set_thinking_level':
  ok()
 elif k=='compact':
  compacts+=1
  if compacts==1:
   emit({'type':'compaction_end','result':{'tokensBefore':150,'estimatedTokensAfter':30}})
   ok({'tokensBefore':150,'estimatedTokensAfter':30})
  else:
   emit({'type':'response','id':m['id'],'command':k,'success':False,'error':'Nothing to compact (session too small)'})
 elif k in ('prompt','steer'):
  ok({'disposition':'started'})
  emit({'type':'message_start','message':{'role':'assistant'}})
  emit({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'answer: '+m['message']}})
  if m['message']=='hold': continue
  emit({'type':'message_end','message':{'role':'assistant','model':'test-model','provider':'test-local','usage':{'totalTokens':21,'input':10,'output':3,'cacheRead':6,'cacheWrite':2}}})
  emit({'type':'agent_settled','aborted':False})
 elif k=='abort':
  emit({'type':'agent_settled','aborted':True})
"#;

    const CLAUDE: &str = r#"#!/usr/bin/python3
import json,sys
a=sys.argv
sid=a[a.index('--session-id')+1] if '--session-id' in a else a[a.index('--resume')+1]
def emit(o): print(json.dumps(o),flush=True)
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')!='user': continue
 emit({'type':'system','subtype':'init','session_id':sid})
 if m['message']['content']=='approve':
  emit({'type':'control_request','request_id':'perm-1','request':{'subtype':'can_use_tool','tool_name':'Bash','input':{'command':'printf ok'}}})
  r=json.loads(next(sys.stdin))['response']
  text='allowed' if r['request_id']=='perm-1' and r['response']['behavior']=='allow' else 'refused'
  emit({'type':'assistant','message':{'id':'msg-1','model':'claude-test','content':[{'type':'text','text':text}]}})
 emit({'type':'result','session_id':sid,'subtype':'success','is_error':False})
"#;

    fn executable(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    struct Cli {
        url: String,
        token_file: Option<PathBuf>,
    }

    impl Cli {
        fn command(&self, args: &[&str]) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate"));
            command.env("PROTECTORATE_HUB", &self.url).args(args);
            match &self.token_file {
                Some(file) => command
                    .env_remove("PROTECTORATE_TOKEN")
                    .env("PROTECTORATE_TOKEN_FILE", file),
                None => command
                    .env("PROTECTORATE_TOKEN", OWNER)
                    .env_remove("PROTECTORATE_TOKEN_FILE"),
            };
            command
        }

        fn run(&self, args: &[&str]) -> Result<String, String> {
            let output = self.command(args).output().unwrap();
            if output.status.success() {
                Ok(String::from_utf8(output.stdout).unwrap())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        }

        fn ok(&self, args: &[&str]) -> String {
            self.run(args)
                .unwrap_or_else(|error| panic!("{args:?}: {error}"))
        }

        fn json(&self, args: &[&str]) -> Value {
            let mut all = vec!["--json"];
            all.extend_from_slice(args);
            serde_json::from_str(&self.ok(&all)).unwrap()
        }

        fn spawn(&self, args: &[&str]) -> Lines {
            let mut child = self
                .command(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, lines) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    if sender.send(line.unwrap()).is_err() {
                        break;
                    }
                }
            });
            Lines {
                child,
                lines,
                seen: Vec::new(),
            }
        }
    }

    /// A running CLI command whose stdout is read line by line.
    struct Lines {
        child: Child,
        lines: mpsc::Receiver<String>,
        seen: Vec<String>,
    }

    impl Lines {
        fn until(&mut self, want: &str) -> String {
            let deadline = Instant::now() + DEADLINE;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.lines.recv_timeout(left) {
                    Ok(line) => {
                        self.seen.push(line.clone());
                        if line.contains(want) {
                            return line;
                        }
                    }
                    Err(_) => panic!("no line with {want:?} in {:?}", self.seen),
                }
            }
        }

        /// Waits for the command to exit by itself and returns its status.
        fn finish(mut self) -> bool {
            let deadline = Instant::now() + DEADLINE;
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    self.seen.extend(self.lines.iter());
                    return status.success();
                }
                assert!(
                    Instant::now() < deadline,
                    "command did not exit: {:?}",
                    self.seen
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    impl Drop for Lines {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn row<'a>(text: &'a str, label: &str) -> &'a str {
        text.lines()
            .find_map(|line| {
                line.strip_prefix(label)
                    .filter(|rest| rest.starts_with([' ', '\t']))
            })
            .unwrap_or_else(|| panic!("no {label:?} row in {text}"))
            .trim()
    }

    pub async fn chat_commands_drive_real_sleeves() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let tools = root.path().join("tools");
        let work = root.path().join("work");
        for dir in [
            &state,
            &tools,
            &work,
            &root.path().join("owner"),
            &root.path().join("runtime"),
            &state.join("auth/pi"),
            &state.join("auth/claude"),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(state.join("auth/claude/.credentials.json"), "{}").unwrap();
        executable(&tools.join("pi"), PI);
        executable(&tools.join("claude"), CLAUDE);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hub = Hub::open(&state, OWNER, "test")
            .unwrap()
            .with_chat_runtime(ChatRuntime::new(ChatLauncher {
                state_root: state.clone(),
                user_home: root.path().join("owner"),
                runtime_dir: root.path().join("runtime"),
                helper: std::env::current_exe().unwrap(),
                pi_executable: Some(tools.join("pi")),
                claude_executable: Some(tools.join("claude")),
                claude_tool_roots: vec![tools.clone()],
                codex_executable: None,
                codex_tool_roots: Vec::new(),
                grok_executable: None,
                grok_tool_roots: Vec::new(),
                tool_roots: vec![tools.clone()],
                flutter_sdk_root: None,
                flutter_cache_lower: None,
                flutter_tools_lower: None,
                pub_cache_lower: None,
                go_mod_cache_lower: None,
                environment: BTreeMap::from([("PROTECTORATE_HUB".into(), url.clone())]),
                tasks_max: 64,
            }));
        let server = tokio::spawn(async move {
            axum::serve(listener, hub.router().unwrap()).await.unwrap();
        });
        tokio::task::spawn_blocking(move || scenario(url, state, work))
            .await
            .unwrap();
        server.abort();
    }

    fn scenario(url: String, state: PathBuf, work: PathBuf) {
        let owner = Cli {
            url: url.clone(),
            token_file: None,
        };
        let sleeve = |name: &str| Cli {
            url: url.clone(),
            token_file: Some(state.join(format!("sleeves/{name}/home/.protectorate-token"))),
        };

        let none = owner.ok(&["orchestrator", "status"]);
        assert_eq!(
            row(&none, "sleeve"),
            "not created (run: protectorate orchestrator spawn)"
        );

        // api call: pretty JSON by default, one compact line with --json.
        let workspace = work.to_str().unwrap();
        let beta_work = work.with_file_name("beta-work");
        let gamma_work = work.with_file_name("gamma-work");
        fs::create_dir_all(&beta_work).unwrap();
        fs::create_dir_all(&gamma_work).unwrap();
        let pretty = owner.ok(&[
            "api",
            "call",
            "SleeveService/SpawnSleeve",
            &json!({"name":"alpha","harness":"HARNESS_PI","workspace":workspace}).to_string(),
        ]);
        assert!(pretty.lines().count() > 3, "{pretty}");
        let spawned: Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(spawned["sleeve"]["name"], "alpha");
        let compact = owner.ok(&[
            "--json",
            "api",
            "call",
            "protectorate.v1.SleeveService/SpawnSleeve",
            &json!({"name":"beta","harness":"HARNESS_PI","workspace":beta_work}).to_string(),
        ]);
        assert_eq!(compact.lines().count(), 1, "{compact}");
        let missing = owner
            .run(&["api", "call", "SleeveService/NoSuchMethod"])
            .unwrap_err();
        assert!(missing.contains("unknown procedure"), "{missing}");
        let invalid = owner
            .run(&["api", "call", "SleeveService/GetSleeve", "{not json"])
            .unwrap_err();
        assert!(invalid.contains("request JSON is invalid"), "{invalid}");

        let mut watch = owner.spawn(&["chat", "watch", "alpha"]);
        owner.ok(&["chat", "send", "alpha", "hello", "alpha"]);
        owner.ok(&["chat", "send", "beta", "hi", "beta"]);
        watch.until("answer: hello alpha");
        let mut idle: Vec<String> = owner
            .ok(&["chat", "wait", "alpha", "beta", "--timeout", "60s"])
            .lines()
            .map(str::to_owned)
            .collect();
        idle.sort();
        assert_eq!(idle, ["alpha\tidle", "beta\tidle"]);

        // A multi-sleeve wait prints each sleeve as its report lands.
        let summary = work.join("summary.md");
        fs::write(&summary, "beta finished first").unwrap();
        let reported = sleeve("beta").ok(&[
            "report",
            "--status",
            "complete",
            "--summary-file",
            summary.to_str().unwrap(),
        ]);
        assert!(reported.starts_with("reported complete"), "{reported}");
        let mut reports = owner.spawn(&[
            "chat",
            "wait",
            "alpha",
            "beta",
            "--until",
            "report",
            "--timeout",
            "80s",
        ]);
        reports.until("beta\tcomplete");
        reports.until("beta finished first");
        fs::write(&summary, "alpha took longer").unwrap();
        sleeve("alpha").ok(&[
            "report",
            "--status",
            "partial",
            "--summary-file",
            summary.to_str().unwrap(),
        ]);
        reports.until("alpha\tpartial");
        reports.until("alpha took longer");
        assert!(reports.finish());

        let models = owner.ok(&["chat", "models", "alpha"]);
        assert!(
            models.contains(
                "test-local/test-alt\tAlternate\tefforts=[off low medium high]\tdefault effort="
            ),
            "{models}"
        );
        let unsupported = owner.ok(&["chat", "models", "--harness", "pi"]);
        assert!(
            !unsupported.trim().is_empty() && !unsupported.contains('\t'),
            "{unsupported}"
        );
        for (args, model, requested) in [
            (
                &["--model", "test-local/test-alt", "--effort", "low"][..],
                "test-alt",
                "test-local/test-alt",
            ),
            (&[][..], "test-alt", "test-local/test-alt"),
            (&["--native-defaults"][..], "test-model", "native default"),
        ] {
            let mut all = vec!["chat", "settings", "alpha"];
            all.extend_from_slice(args);
            let settings = owner.ok(&all);
            assert_eq!(row(&settings, "model"), model, "{args:?}: {settings}");
            assert_eq!(
                row(&settings, "requested model"),
                requested,
                "{args:?}: {settings}"
            );
            assert_eq!(row(&settings, "native model"), "test-local/test-model");
        }

        assert_eq!(
            owner.ok(&["chat", "compact", "alpha"]),
            "compacted\t150 -> 30 tokens\n"
        );
        let small = owner.run(&["chat", "compact", "alpha"]).unwrap_err();
        assert!(small.contains("Nothing to compact"), "{small}");

        let status = owner.ok(&["chat", "status", "alpha"]);
        assert_eq!(row(&status, "chat state"), "idle", "{status}");
        assert_eq!(row(&status, "context tokens"), "21/8192", "{status}");
        assert!(row(&status, "cache total").starts_with("1 calls, in 10 out 3 read 6 write 2"));
        assert!(row(&status, "cache first call").ends_with("fresh"));
        assert!(!status.contains("cache expires"), "{status}");
        let chat = owner.json(&["chat", "status", "alpha"]);
        assert_eq!(chat["cache"]["firstCall"]["cacheReadTokens"], "6", "{chat}");

        owner.ok(&["chat", "send", "alpha", "hold"]);
        watch.until("owner: hold");
        assert_eq!(owner.ok(&["chat", "interrupt", "alpha"]), "interrupted\n");
        // A streamed answer stays on its line until the next event ends it.
        watch.until("answer: hold");
        watch.until("settled   cancelled");
        let waited = owner.ok(&["chat", "wait", "alpha", "--timeout", "60s"]);
        assert!(waited.starts_with("alpha\t"), "{waited}");

        owner.ok(&[
            "api",
            "call",
            "SleeveService/SpawnSleeve",
            &json!({"name":"gamma","harness":"HARNESS_CLAUDE","workspace":gamma_work,"supervised":true})
                .to_string(),
        ]);
        let mut gate = owner.spawn(&["chat", "watch", "gamma"]);
        owner.ok(&["chat", "send", "gamma", "approve"]);
        gate.until("approval  perm-1");
        assert_eq!(
            owner.ok(&["chat", "respond", "gamma", "perm-1", "--allow"]),
            "answered\n"
        );
        gate.until("approvalResolved perm-1 allow=true by owner");
        gate.until("assistant allowed");

        // The audit log filters by the run a sleeve's calls belong to.
        let run = owner.json(&["sleeve", "show", "alpha"])["sleeve"]["runId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!run.is_empty());
        let audited = owner.ok(&["audit", "ls", "--run", &run]);
        assert!(
            audited.contains("ChatService/Report")
                && audited.lines().all(|line| line.contains(&run)),
            "{audited}"
        );
        assert!(!audited.contains("beta finished first"));

        owner.ok(&["orchestrator", "spawn", "--harness", "pi"]);
        let envoy = owner.ok(&["orchestrator", "status"]);
        assert!(
            row(&envoy, "sleeve").starts_with("envoy (running"),
            "{envoy}"
        );
        assert_eq!(row(&envoy, "harness"), "pi", "{envoy}");
        assert!(row(&envoy, "token").ends_with("(active)"), "{envoy}");
        assert!(!row(&envoy, "scopes").is_empty(), "{envoy}");

        // Watching ends cleanly when its sleeve is killed.
        owner.ok(&["sleeve", "kill", "alpha"]);
        assert!(watch.finish());
        for name in ["beta", "gamma", "envoy"] {
            owner.ok(&["sleeve", "kill", name]);
        }
    }
}
