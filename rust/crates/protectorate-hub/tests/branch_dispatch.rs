#![cfg(target_os = "linux")]
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn call(addr: std::net::SocketAddr, body: Value) -> (u16, Value) {
    call_path(addr, "SleeveService/SpawnSleeve", body).await
}

async fn call_path(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn branch_dispatch_clones_and_rolls_back_failed_launch() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let runtime = root.path().join("runtime");
    let tools = root.path().join("tools");
    for dir in [
        &source,
        &state,
        &owner,
        &runtime,
        &tools,
        &state.join("auth/pi"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    let status = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    git(&source, &["config", "user.name", "Test"]);
    git(&source, &["config", "user.email", "test@example.test"]);
    fs::write(source.join("file.txt"), "source\n").unwrap();
    git(&source, &["add", "file.txt"]);
    git(&source, &["commit", "-qm", "initial"]);
    let tip = git(&source, &["rev-parse", "HEAD"]);
    let pi = tools.join("pi");
    fs::write(&pi, r#"#!/usr/bin/python3
import json,os,subprocess,sys
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='get_state':
  base=subprocess.run(['git','remote','get-url','base'],capture_output=True,text=True)
  if base.returncode==0:
   path=base.stdout.strip()
   source=os.path.dirname(os.readlink(path+'/objects'))
   fetched=subprocess.run(['git','fetch','-q','base'],capture_output=True,text=True)
   with open('base-config-proof','w') as f:
    f.write(open(path+'/config').read())
   with open('base-proof','w') as f:
    f.write('|'.join([str(os.path.exists(source+'/config')),str(os.path.exists(source+'/logs')),str(os.path.exists(source+'/FETCH_HEAD')),str(fetched.returncode),fetched.stderr]))
  print(json.dumps({'type':'response','id':m['id'],'command':'get_state','success':True,'data':{'sessionId':'branch-test','model':{'id':'test','provider':'local','contextWindow':8192},'thinkingLevel':'medium'}}),flush=True)
 elif m.get('type')=='get_available_models':
  print(json.dumps({'type':'response','id':m['id'],'command':'get_available_models','success':True,'data':{'models':[{'id':'test','provider':'local','name':'Test','reasoning':True}]}}),flush=True)
 elif m.get('type')=='get_available_thinking_levels':
  print(json.dumps({'type':'response','id':m['id'],'command':'get_available_thinking_levels','success':True,'data':{'levels':['off','low','medium','high']}}),flush=True)
 elif m.get('type')=='set_model':
  print(json.dumps({'type':'response','id':m['id'],'command':'set_model','success':True,'data':{'id':m['modelId'],'provider':m['provider']}}),flush=True)
 elif m.get('type')=='set_thinking_level':
  print(json.dumps({'type':'response','id':m['id'],'command':'set_thinking_level','success':True}),flush=True)
 elif m.get('type') in ('prompt','steer'):
  base=subprocess.run(['git','remote','get-url','base'],capture_output=True,text=True)
  path=base.stdout.strip()
  source=os.path.dirname(os.readlink(path+'/objects'))
  fetched=subprocess.run(['git','fetch','-q','base'],capture_output=True,text=True)
  with open('notice-fetch-proof','w') as f:
   f.write('|'.join([str(fetched.returncode),str(os.path.exists(source+'/config.worktree')),str(os.path.exists(source+'/FETCH_HEAD')),str(os.path.exists(source+'/logs/new-secret')),fetched.stderr]))
  print(json.dumps({'type':'response','id':m['id'],'command':m['type'],'success':True,'data':{'disposition':'started'}}),flush=True)
  print(json.dumps({'type':'agent_settled','aborted':False}),flush=True)
"#).unwrap();
    fs::set_permissions(&pi, fs::Permissions::from_mode(0o755)).unwrap();
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        codex_executable: None,
        codex_tool_roots: Vec::new(),
        grok_executable: None,
        grok_tool_roots: Vec::new(),
        claude_tool_roots: Vec::new(),
        tool_roots: vec![tools],
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        environment: BTreeMap::new(),
        tasks_max: 64,
    };
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let request = json!({"name":"branch-one","harness":"HARNESS_PI","workspace":source,"branch":"main","newBranch":"feature/test"});
    let (status, response) = call(addr, request.clone()).await;
    assert_eq!(status, 200, "{response}");
    let destination = state.join("workspaces/branch-one");
    assert_eq!(response["sleeve"]["workspace"], json!(destination));
    assert_eq!(response["sleeve"]["baseRepo"], json!(source));
    assert_eq!(response["sleeve"]["baseBranch"], "main");
    assert_eq!(response["sleeve"]["baseTip"], tip);
    assert_eq!(
        git(&destination, &["branch", "--show-current"]),
        "feature/test"
    );
    assert_eq!(
        fs::read_to_string(destination.join("file.txt")).unwrap(),
        "source\n"
    );
    assert_eq!(
        git(
            &destination,
            &["status", "--porcelain", "--untracked-files=no"]
        ),
        ""
    );
    assert_eq!(git(&source, &["branch", "--show-current"]), "main");
    assert_eq!(
        git(&destination, &["remote", "get-url", "base"]),
        state.join("base-views/branch-one").to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(source.join("file.txt")).unwrap(),
        "source\n"
    );
    assert_eq!(
        fs::read_to_string(destination.join("base-proof")).unwrap(),
        "False|False|False|0|"
    );
    fs::write(source.join(".git/config.worktree"), "late-secret\n").unwrap();
    fs::write(source.join(".git/FETCH_HEAD"), "late-secret\n").unwrap();
    fs::write(source.join(".git/logs/new-secret"), "late-secret\n").unwrap();
    fs::write(source.join("next.txt"), "advance\n").unwrap();
    git(&source, &["add", "next.txt"]);
    git(&source, &["commit", "-qm", "advance"]);
    let next_tip = git(&source, &["rev-parse", "HEAD"]);
    git(&source, &["pack-refs", "--all", "--prune"]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    loop {
        let (status, inbox) = call_path(
            addr,
            "MessageService/Inbox",
            json!({"as":"branch-one","peek":true}),
        )
        .await;
        assert_eq!(status, 200, "{inbox}");
        if inbox["messages"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["body"].as_str().is_some_and(|body| {
                    body.contains("Base branch main advanced")
                        && body.contains("git fetch base && git rebase base/main")
                }) && item["deliveries"][0]["deliveredAt"].is_string()
            })
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "base notice did not arrive: {inbox}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let (status, updated) = call_path(
        addr,
        "SleeveService/GetSleeve",
        json!({"name":"branch-one"}),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["sleeve"]["baseTip"], next_tip);
    let fetch_proof = destination.join("notice-fetch-proof");
    let proof_deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while !fetch_proof.exists() && std::time::Instant::now() < proof_deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        fs::read_to_string(fetch_proof).unwrap(),
        "0|False|False|False|"
    );
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let (status, deduped) = call_path(
        addr,
        "MessageService/Inbox",
        json!({"as":"branch-one","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{deduped}");
    assert_eq!(deduped["messages"].as_array().unwrap().len(), 1);
    let view = state.join("base-views/branch-one");
    let saved_view = state.join("base-views/branch-one-saved");
    let outside = root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("packed-refs"), "untouched\n").unwrap();
    fs::rename(&view, &saved_view).unwrap();
    std::os::unix::fs::symlink(&outside, &view).unwrap();
    git(&source, &["tag", "late-tag"]);
    git(&source, &["pack-refs", "--all", "--prune"]);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert_eq!(
        fs::read_to_string(outside.join("packed-refs")).unwrap(),
        "untouched\n"
    );
    fs::remove_file(&view).unwrap();
    fs::rename(saved_view, view).unwrap();

    let (duplicate_status, duplicate) = call(addr, request).await;
    assert_eq!(duplicate_status, 409, "{duplicate}");
    let (bad_status, bad) = call(
        addr,
        json!({"name":"bad","harness":"HARNESS_PI","workspace":source,"newBranch":"../bad"}),
    )
    .await;
    assert_eq!(bad_status, 400, "{bad}");
    assert!(!state.join("workspaces/bad").exists());
    let (failed_status, failed) = call(addr, json!({"name":"launch-fails","harness":"HARNESS_SHELL","workspace":source,"newBranch":"feature/fail"})).await;
    assert_eq!(failed_status, 503, "{failed}");
    assert!(!state.join("workspaces/launch-fails").exists());
    assert!(!state.join("base-views/launch-fails").exists());
    let url = format!("file://{}", source.display());
    let (url_status, url_spawned) = call(
        addr,
        json!({"name":"url-clone","harness":"HARNESS_PI","workspace":url}),
    )
    .await;
    assert_eq!(url_status, 200, "{url_spawned}");
    let url_clone = state.join("workspaces/url-clone");
    assert_eq!(git(&url_clone, &["branch", "--show-current"]), "main");
    assert_eq!(git(&url_clone, &["remote", "get-url", "origin"]), url);
    assert_eq!(
        fs::read_to_string(url_clone.join("file.txt")).unwrap(),
        "source\n"
    );
    assert!(url_spawned["sleeve"]["baseRepo"].is_null());
    let sha_source = root.path().join("sha-source");
    let status = Command::new("git")
        .args(["init", "-q", "-b", "main", "--object-format=sha256"])
        .arg(&sha_source)
        .status()
        .unwrap();
    assert!(status.success());
    git(&sha_source, &["config", "user.name", "Test"]);
    git(&sha_source, &["config", "user.email", "test@example.test"]);
    git(
        &sha_source,
        &["config", "protectorate.private", "hidden-secret"],
    );
    fs::write(sha_source.join("file.txt"), "sha256\n").unwrap();
    git(&sha_source, &["add", "file.txt"]);
    git(&sha_source, &["commit", "-qm", "sha256 initial"]);
    let (sha_status, sha_spawned) = call(addr, json!({"name":"sha-clone","harness":"HARNESS_PI","workspace":sha_source,"newBranch":"feature/sha"})).await;
    assert_eq!(sha_status, 200, "{sha_spawned}");
    let sha_clone = state.join("workspaces/sha-clone");
    assert_eq!(
        fs::read_to_string(sha_clone.join("base-config-proof")).unwrap(),
        "[core]\nrepositoryformatversion = 1\nbare = true\n[extensions]\nobjectFormat = sha256\n"
    );
    assert!(fs::read_to_string(sha_clone.join("base-proof"))
        .unwrap()
        .ends_with("|0|"));
    task.abort();
}
