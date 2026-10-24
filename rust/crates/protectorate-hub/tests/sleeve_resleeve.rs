#![cfg(target_os = "linux")]
use crate::support::executable;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path, process::Command,
    sync::Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}
async fn call(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}
fn executable(path: &Path, script: &str) {
    executable::write_executable(path, script);
}
fn git(workspace: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn fake_pi(path: &Path) {
    executable(
        path,
        r#"#!/usr/bin/python3
import json,os,sys
for line in sys.stdin:
 m=json.loads(line); k=m.get('type')
 if k=='get_state':
  if os.getenv('TASK_TAG'): open('pi-env-proof','w').write(os.environ['TASK_TAG']+'|'+os.environ['GOMAXPROCS'])
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'sessionId':'pi-original','model':{'id':'local-test','provider':'local','baseUrl':'http://127.0.0.1:1234/v1','contextWindow':8192},'thinkingLevel':'medium'}}),flush=True)
 elif k=='get_available_models':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'models':[{'id':'local-test','provider':'local','name':'Local Test','reasoning':True}]}}),flush=True)
 elif k=='get_available_thinking_levels':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'levels':['off','low','medium','high']}}),flush=True)
 elif k=='set_model':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'id':m['modelId'],'provider':m['provider'],'baseUrl':'http://127.0.0.1:1234/v1'}}),flush=True)
 elif k=='set_thinking_level':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True}),flush=True)
 elif k in ('prompt','steer'):
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'disposition':'started'}}),flush=True)
  print(json.dumps({'type':'message_start','message':{'role':'assistant'}}),flush=True)
  txt=m['message']
  if 'Write the summary part of a handoff note' in txt:
   answer='' if 'summary-fail' in txt else '## Goal\n- Continue implementation\n## State\n- Facts reviewed\n## Unfinished\n- Verify changes\n## Next\n- Read the handoff\n## Gotchas\n- Scratch='+str(os.path.exists('task.txt'))+' token='+str(bool(os.getenv('PROTECTORATE_TOKEN_FILE')))
  else: answer='pi answer: '+txt
  if answer: print(json.dumps({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':answer}}),flush=True)
  print(json.dumps({'type':'message_end','message':{'role':'assistant','model':'local-test','provider':'local','usage':{'totalTokens':8,'input':5,'output':3}}}),flush=True)
  print(json.dumps({'type':'agent_settled','aborted':False}),flush=True)
 elif k=='abort': print(json.dumps({'type':'agent_settled','aborted':True}),flush=True)
"#,
    );
}
fn fake_codex(path: &Path) {
    executable(
        path,
        r#"#!/usr/bin/python3
import json,os,sys
if os.getenv('TASK_TAG'): open('codex-env-proof','w').write(os.environ['TASK_TAG']+'|'+os.environ['GOMAXPROCS'])
def emit(x): print(json.dumps(x),flush=True)
def note(method,params): params['threadId']='codex-new'; emit({'method':method,'params':params})
turn=0
for line in sys.stdin:
 m=json.loads(line); method=m.get('method'); mid=m.get('id'); p=m.get('params',{})
 if method=='initialize': emit({'id':mid,'result':{}})
 elif method=='initialized': pass
 elif method=='thread/start': emit({'id':mid,'result':{'thread':{'id':'codex-new'},'model':'gpt-test','reasoningEffort':'medium'}})
 elif method=='thread/resume': emit({'id':mid,'result':{'thread':{'id':'codex-new'},'model':'gpt-test','reasoningEffort':'medium'}})
 elif method=='model/list':
  emit({'id':mid,'result':{'data':[{'model':'gpt-test','displayName':'Test','isDefault':True,'defaultReasoningEffort':'medium','supportedReasoningEfforts':[{'reasoningEffort':'medium'},{'reasoningEffort':'high'}]}]}})
 elif method=='thread/settings/update':
  emit({'id':mid,'result':{}})
  note('thread/settings/updated',{'threadSettings':{'model':p['model'],'effort':p.get('effort','medium')}})
 elif method=='turn/start':
  turn+=1; txt=p['input'][0]['text']; emit({'id':mid,'result':{'turn':{'id':'t'+str(turn)}}})
  note('turn/started',{'turn':{'id':'t'+str(turn)}})
  note('item/agentMessage/delta',{'itemId':'m1','delta':'codex answer: '+txt})
  note('turn/completed',{'turn':{'id':'t'+str(turn),'status':'completed'}})
 elif method=='thread/unsubscribe': emit({'id':mid,'result':{}})
"#,
    );
}
fn fixture(with_codex: bool) -> (tempfile::TempDir, Hub, std::path::PathBuf) {
    fixture_with_pi(with_codex, None)
}

fn fixture_with_pi(
    with_codex: bool,
    live_pi: Option<(&Path, &Path)>,
) -> (tempfile::TempDir, Hub, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let work = root.path().join("work");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [
        &state,
        &owner,
        &work,
        &tools,
        &runtime,
        &state.join("auth/pi"),
        &state.join("auth/codex"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(work.join("task.txt"), "unfinished work").unwrap();
    fs::write(state.join("auth/codex/auth.json"), "{}").unwrap();
    let pi = if let Some((executable, auth)) = live_pi {
        fs::copy(auth, state.join("auth/pi/auth.json")).unwrap();
        for file in ["models.json", "settings.json"] {
            let source = auth.parent().unwrap().join(file);
            if source.is_file() {
                fs::copy(source, state.join("auth/pi").join(file)).unwrap();
            }
        }
        executable.to_path_buf()
    } else {
        let pi = tools.join("pi");
        fake_pi(&pi);
        pi
    };
    let codex = tools.join("codex");
    if with_codex {
        fake_codex(&codex);
    }
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state,
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: Some(pi),
            claude_executable: None,
            claude_tool_roots: vec![],
            codex_executable: with_codex.then_some(codex),
            codex_tool_roots: vec![tools.clone()],
            grok_executable: None,
            grok_tool_roots: vec![],
            tool_roots: live_pi
                .map(|(executable, _)| {
                    vec![
                        tools.clone(),
                        executable.parent().unwrap().to_path_buf(),
                        executable
                            .parent()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("share/pi-omp-setup"),
                    ]
                })
                .unwrap_or_else(|| vec![tools]),
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    (root, hub, work)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_environment_is_private_and_survives_resleeve() {
    let (root, hub, workspace) = fixture(true);
    let (addr, server) = serve(hub).await;
    for key in [
        "1INVALID",
        "HOME",
        "PROTECTORATE_TOKEN",
        "PROTECTORATE_TOKEN_FILE",
    ] {
        let (status, rejected) = call(
            addr,
            "SleeveService/SpawnSleeve",
            "owner-secret",
            json!({"name":"task","harness":"HARNESS_PI","workspace":workspace,
                "env":{key:"blocked"}}),
        )
        .await;
        assert_eq!(status, 400, "{key}: {rejected}");
        assert!(!workspace.join("pi-env-proof").exists());
    }
    let (status, rejected) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({"name":"task","harness":"HARNESS_PI","workspace":workspace,
            "env":{"TASK_TAG":"has\u{0000}nul"}}),
    )
    .await;
    assert_eq!(status, 400, "{rejected}");
    let marker = "private-env-value";
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({"name":"task","harness":"HARNESS_PI","workspace":workspace,
            "env":{"TASK_TAG":marker,"GOMAXPROCS":"3"}}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(
        fs::read_to_string(workspace.join("pi-env-proof")).unwrap(),
        format!("{marker}|3")
    );
    assert!(!spawned.to_string().contains(marker));
    let registry = root.path().join("state/sleeves.json");
    assert_eq!(
        fs::metadata(&registry).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let reopened = protectorate_sleeve_registry::Registry::open(&registry).unwrap();
    assert_eq!(reopened.get("task").unwrap().env["TASK_TAG"], marker);

    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({"name":"task","to":"HARNESS_CODEX","nativeDefaults":true}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(
        fs::read_to_string(workspace.join("codex-env-proof")).unwrap(),
        format!("{marker}|3")
    );
    assert!(!moved.to_string().contains(marker));
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moves_pi_to_codex_with_handoff_history_and_rotated_identity() {
    let (root, hub, workspace) = fixture(true);
    git(&workspace, &["init", "-q"]);
    git(
        &workspace,
        &["symbolic-ref", "HEAD", "refs/heads/handoff-test"],
    );
    git(&workspace, &["add", "task.txt"]);
    git(
        &workspace,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "Start task",
        ],
    );
    let host_marker = root.path().join("host-only-git-filter-marker");
    let filter = workspace.join("filter.sh");
    executable(
        &filter,
        &format!(
            "#!/bin/sh\ntouch \"$HOME/ran-git-filter\"\ntouch '{}' 2>/dev/null || :\ncat\n",
            host_marker.display()
        ),
    );
    fs::write(
        workspace.join(".gitattributes"),
        "task.txt filter=hostprobe\n",
    )
    .unwrap();
    git(
        &workspace,
        &["config", "filter.hostprobe.clean", filter.to_str().unwrap()],
    );
    fs::write(workspace.join("task.txt"), "unfinished work\nlocal edit\n").unwrap();
    assert!(!host_marker.exists());
    let records = Arc::clone(&hub.sleeves);
    let (addr, server) = serve(hub).await;
    let (status,spawned)=call(addr,"SleeveService/SpawnSleeve","owner-secret",json!({"name":"task","harness":"HARNESS_PI","workspace":workspace,"brief":"implement the task"})).await;
    assert_eq!(status, 200, "{spawned}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        "owner-secret",
        json!({"sleeve":"task","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    records
        .set_result(
            "task",
            json!({"status":"REPORT_STATUS_PARTIAL","summary":"first stage done"}),
        )
        .unwrap();
    let token_path = root
        .path()
        .join("state/sleeves/task/home/.protectorate-token");
    let old_token = fs::read_to_string(&token_path).unwrap();
    let (status, forbidden) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        old_token.trim(),
        json!({"name":"task","to":"HARNESS_CODEX","nativeDefaults":true}),
    )
    .await;
    assert_eq!(status, 403, "{forbidden}");
    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({"name":"task","to":"HARNESS_CODEX","nativeDefaults":true}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["sleeve"]["harness"], "HARNESS_CODEX");
    assert_eq!(moved["sleeve"]["workspace"], json!(workspace));
    assert_eq!(moved["sleeve"]["result"]["summary"], "first stage done");
    assert_eq!(moved["sleeve"]["handoff"]["fromHarness"], "pi");
    assert_eq!(moved["sleeve"]["handoff"]["fromSessionId"], "pi-original");
    assert_eq!(moved["sleeve"]["handoff"]["toHarness"], "codex");
    assert_eq!(moved["sleeve"]["handoff"]["summarizerHarness"], "pi");
    assert_eq!(moved["sleeve"]["handoff"]["summarizerModel"], "local-test");
    assert_eq!(moved["sleeve"]["handoff"]["summarizerProvider"], "local");
    assert_eq!(moved["sleeve"]["handoff"]["summarizerLocal"], true);
    let handoff = fs::read_to_string(workspace.join(".protectorate/HANDOFF.md")).unwrap();
    assert!(handoff.contains("pi local local-test"), "{handoff}");
    assert!(
        handoff.contains("a local model: pi local local-test"),
        "{handoff}"
    );
    assert!(handoff.contains("Continue implementation"), "{handoff}");
    assert!(handoff.contains("Scratch=False token=False"), "{handoff}");
    assert!(handoff.contains("implement the task"), "{handoff}");
    assert!(handoff.contains("first stage done"), "{handoff}");
    assert!(handoff.contains("handoff-test"), "{handoff}");
    assert!(handoff.contains(" M task.txt"), "{handoff}");
    assert!(handoff.contains("Start task"), "{handoff}");
    assert!(handoff.contains("git diff --stat HEAD"), "{handoff}");
    assert!(
        !host_marker.exists(),
        "Git filter wrote outside the sandbox"
    );
    assert!(
        root.path()
            .join("state/helpers/git-task/home/ran-git-filter")
            .exists(),
        "the filter must execute inside the private helper sandbox"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("task.txt")).unwrap(),
        "unfinished work\nlocal edit\n"
    );
    let events =
        protectorate_chatlog::read_all(root.path().join("state/sleeves/task/chat.jsonl")).unwrap();
    assert!(events
        .iter()
        .any(|event| event.payload["handoff"]["toHarness"] == "codex"));
    assert!(events.iter().any(|event| event.payload["user"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("HANDOFF.md"))));
    let (status, old) = call(
        addr,
        "SleeveService/ListSleeves",
        old_token.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 401, "{old}");
    let new_token = fs::read_to_string(&token_path).unwrap();
    let (status, current) = call(
        addr,
        "SleeveService/ListSleeves",
        new_token.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{current}");
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_chat_spawns_with_delegation_and_preserves_it_on_resleeve() {
    let (_root, hub, workspace) = fixture(true);
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({
            "name":"task","harness":"HARNESS_PI","workspace":workspace,
            "nested":true,"limits":{"cpuPercent":100,"memoryBytes":"268435456"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["nested"], true);
    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({"name":"task","to":"HARNESS_CODEX","nativeDefaults":true}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["sleeve"]["nested"], true);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_target_launch_restores_original_pi_session_and_token() {
    let (root, hub, workspace) = fixture(false);
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({"name":"task","harness":"HARNESS_PI","workspace":workspace,"brief":"resume me"}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        "owner-secret",
        json!({"sleeve":"task","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let token_path = root
        .path()
        .join("state/sleeves/task/home/.protectorate-token");
    let old_token = fs::read_to_string(&token_path).unwrap();
    let (status, rejected) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({"name":"task","to":"HARNESS_SHELL"}),
    )
    .await;
    assert_eq!(status, 400, "{rejected}");
    assert!(!workspace.join(".protectorate/HANDOFF.md").exists());
    let (status, failed) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({"name":"task","to":"HARNESS_GROK","nativeDefaults":true}),
    )
    .await;
    assert_eq!(status, 400, "{failed}");
    assert!(
        failed["message"]
            .as_str()
            .unwrap()
            .contains("back on its own session"),
        "{failed}"
    );
    let (status, current) = call(
        addr,
        "SleeveService/GetSleeve",
        "owner-secret",
        json!({"name":"task"}),
    )
    .await;
    assert_eq!(status, 200, "{current}");
    assert_eq!(current["sleeve"]["harness"], "HARNESS_PI");
    assert_eq!(current["sleeve"]["state"], "SLEEVE_STATE_RUNNING");
    assert_eq!(current["sleeve"]["chat"]["harnessSessionId"], "pi-original");
    let (status, old) = call(
        addr,
        "SleeveService/ListSleeves",
        old_token.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 401, "{old}");
    let new_token = fs::read_to_string(token_path).unwrap();
    let (status, new) = call(
        addr,
        "SleeveService/ListSleeves",
        new_token.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{new}");
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_summary_keeps_the_fact_only_handoff() {
    let (_root, hub, workspace) = fixture(false);
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({
            "name":"task", "harness":"HARNESS_PI", "workspace":workspace,
            "brief":"summary-fail: finish the task"
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        "owner-secret",
        json!({
            "sleeve":"task", "until":"WAIT_UNTIL_IDLE", "timeout":"60s"
        }),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({
            "name":"task", "to":"HARNESS_PI"
        }),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert!(moved["sleeve"]["handoff"]["summaryError"]
        .as_str()
        .unwrap()
        .contains("empty summary"));
    let handoff = fs::read_to_string(workspace.join(".protectorate/HANDOFF.md")).unwrap();
    assert!(handoff.contains("No summary:"), "{handoff}");
    assert!(
        handoff.contains("summary-fail: finish the task"),
        "{handoff}"
    );
    server.abort();
}

/// Run manually with PROTECTORATE_LIVE_PI and PROTECTORATE_LIVE_PI_AUTH.
/// The credential is copied only into this test's disposable hub state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an installed authenticated Pi model"]
async fn authenticated_pi_summarizes_a_disposable_codex_handoff() {
    let pi = std::env::var("PROTECTORATE_LIVE_PI").unwrap();
    let auth = std::env::var("PROTECTORATE_LIVE_PI_AUTH").unwrap();
    let (_root, hub, workspace) = fixture_with_pi(true, Some((Path::new(&pi), Path::new(&auth))));
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({
            "name":"task", "harness":"HARNESS_CODEX", "workspace":workspace,
            "brief":"Say ready; this is a disposable handoff probe."
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        "owner-secret",
        json!({
            "sleeve":"task", "until":"WAIT_UNTIL_IDLE", "timeout":"60s"
        }),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        "owner-secret",
        json!({
            "name":"task", "to":"HARNESS_CODEX"
        }),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert!(moved["sleeve"]["handoff"]["summaryError"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    let handoff = fs::read_to_string(workspace.join(".protectorate/HANDOFF.md")).unwrap();
    assert!(handoff.contains("Summarized by"), "{handoff}");
    assert!(handoff.contains("### Goal"), "{handoff}");
    server.abort();
}
