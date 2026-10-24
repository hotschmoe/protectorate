#![cfg(target_os = "linux")]
#[path = "support/executable.rs"]
mod executable;
#[path = "support/private_tmux.rs"]
mod private_tmux;
use private_tmux::PrivateTmux;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub, TerminalLauncher};
use protectorate_session::Manager;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_agent_harnesses_use_their_tui_with_scoped_credentials() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner_home = root.path().join("owner");
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    let tools = root.path().join("tools");
    for dir in [&state, &owner_home, &workspace, &runtime, &tools] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        owner_home.join(".gitconfig"),
        "[user]\n\tname = Owner \\\"Q\\\" Name\n\temail = owner@example.com\n",
    )
    .unwrap();
    let mut executables = Vec::new();
    for provider in ["pi", "claude", "codex", "grok"] {
        let auth = state.join("auth").join(provider);
        fs::create_dir_all(&auth).unwrap();
        fs::write(auth.join("token"), provider).unwrap();
        let executable = tools.join(provider);
        executable::write_executable(
            &executable,
            "#!/usr/bin/sh\nfor other in pi claude codex grok; do if [ \"$other\" != \"$PROTECTORATE_SLEEVE\" ] && [ -e \"/run/protectorate/auth/$other\" ]; then echo credential-leak > \"proof-$PROTECTORATE_SLEEVE\"; exit 1; fi; done\nprintf '%s|%s|%s|%s|%s|%s|%s <%s>|%s|%s %s %s %s' \"$*\" \"$(cat /run/protectorate/auth/$PROTECTORATE_SLEEVE/token)\" \"${PI_CODING_AGENT_DIR:-}\" \"${CLAUDE_CONFIG_DIR:-}\" \"${CODEX_HOME:-}\" \"${GROK_HOME:-}\" \"$(git config user.name)\" \"$(git config user.email)\" \"${DISABLE_AUTOUPDATER:-}\" \"${GOMAXPROCS:-}\" \"${GOFLAGS:-}\" \"${CARGO_BUILD_JOBS:-}\" \"${FLUTTER_TEST_CONCURRENCY:-}\" > \"proof-$PROTECTORATE_SLEEVE\"\nexec /usr/bin/sh\n",
        );
        executables.push(executable);
    }
    let socket = format!("p-rust-agent-tui-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let helper = std::path::PathBuf::from(env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper"));
    let terminal = TerminalLauncher {
        state_root: state.clone(),
        user_home: owner_home.clone(),
        runtime_dir: runtime.clone(),
        runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
        helper: helper.clone(),
        hub_url: format!("http://{addr}"),
        cli_executable: None,
        tool_roots: Vec::new(),
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        tasks_max: 64,
    };
    let chat = ChatLauncher {
        state_root: state.clone(),
        user_home: owner_home,
        runtime_dir: runtime,
        helper,
        pi_executable: Some(executables[0].clone()),
        claude_executable: Some(executables[1].clone()),
        claude_tool_roots: vec![tools.clone()],
        codex_executable: Some(executables[2].clone()),
        codex_tool_roots: vec![tools.clone()],
        grok_executable: Some(executables[3].clone()),
        grok_tool_roots: vec![tools.clone()],
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
        .with_sessions(Arc::clone(&manager))
        .with_terminal_launcher(terminal)
        .with_chat_runtime(ChatRuntime::new(chat));
    let (_, server) = serve(listener, hub).await;
    for (provider, harness) in [
        ("pi", "HARNESS_PI"),
        ("claude", "HARNESS_CLAUDE"),
        ("codex", "HARNESS_CODEX"),
        ("grok", "HARNESS_GROK"),
    ] {
        let (status, spawned) = call(
            addr,
            "/protectorate.v1.SleeveService/SpawnSleeve",
            json!({"name":provider,"harness":harness,"mode":"SLEEVE_MODE_TERMINAL","workspace":workspace,"args":["--example"],
                "limits":{"cpuPercent":250},"env":{"CARGO_BUILD_JOBS":"1"}}),
        )
        .await;
        assert_eq!(status, 200, "{provider}: {spawned}");
        assert_eq!(spawned["sleeve"]["harness"], harness);
        assert_eq!(spawned["sleeve"]["mode"], "SLEEVE_MODE_TERMINAL");
        let proof = workspace.join(format!("proof-{provider}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !proof.exists() {
            assert!(Instant::now() < deadline, "{provider} TUI did not start");
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let result = fs::read_to_string(&proof).unwrap();
        assert!(result.contains(&format!("|{provider}|")), "{result}");
        let fields = result.split('|').collect::<Vec<_>>();
        let provider_env = match provider {
            "pi" => "/run/protectorate/auth/pi".to_owned(),
            "claude" => root
                .path()
                .join("owner/.claude")
                .to_string_lossy()
                .into_owned(),
            "codex" => root
                .path()
                .join("owner/.codex")
                .to_string_lossy()
                .into_owned(),
            "grok" => root
                .path()
                .join("owner/.grok")
                .to_string_lossy()
                .into_owned(),
            _ => unreachable!(),
        };
        let env_index = match provider {
            "pi" => 2,
            "claude" => 3,
            "codex" => 4,
            "grok" => 5,
            _ => unreachable!(),
        };
        assert_eq!(fields[env_index], provider_env, "{provider}: {result}");
        assert_eq!(
            fields[6], "Owner \"Q\" Name <owner@example.com>",
            "{provider}: {result}"
        );
        let claude = provider == "claude";
        assert_eq!(fields[7], if claude { "1" } else { "" }, "{result}");
        // A 2.5-core quota sizes worker pools to two; the caller's value wins.
        assert_eq!(fields[8], "2 -p=2 1 2", "{result}");
        assert_eq!(
            state
                .join("sleeves")
                .join(provider)
                .join("home/.claude/.claude.json")
                .exists(),
            claude
        );
        assert!(
            result.starts_with(if provider == "codex" {
                "--disable daemon_auto_start --example"
            } else {
                "--example"
            }),
            "{result}"
        );
        let (status, killed) = call(
            addr,
            "/protectorate.v1.SleeveService/KillSleeve",
            json!({"name":provider}),
        )
        .await;
        assert_eq!(status, 200, "{provider}: {killed}");
    }
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_tool_roots_run_go_without_exposing_owner_credentials() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner_home = root.path().join("owner");
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    let go = owner_home.join(".local/go/bin/go");
    for dir in [
        &state,
        &workspace,
        &runtime,
        go.parent().unwrap(),
        &owner_home.join(".cargo"),
        &owner_home.join(".codex"),
        &owner_home.join("go/pkg/mod/cache/vcs/private/.git"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(owner_home.join(".cargo/credentials.toml"), b"secret").unwrap();
    fs::write(owner_home.join(".codex/auth.json"), b"secret").unwrap();
    fs::write(
        owner_home.join("go/pkg/mod/cache/vcs/private/.git/config"),
        b"url = https://token@example.invalid/private.git",
    )
    .unwrap();
    let cargo_secret = owner_home.join(".cargo/credentials.toml");
    let codex_secret = owner_home.join(".codex/auth.json");
    let go_secret = owner_home.join("go/pkg/mod/cache/vcs/private/.git/config");
    executable::write_executable(
        &go,
        &format!(
            "#!/usr/bin/sh\nif [ -e '{cargo_secret}' ] || [ -e '{codex_secret}' ] || [ -e '{go_secret}' ]; then printf leak > proof; exit 1; fi\nprintf 'go-visible:%s' \"$PATH\" > proof\nexec /usr/bin/sh\n",
            cargo_secret = cargo_secret.display(),
            codex_secret = codex_secret.display(),
            go_secret = go_secret.display(),
        ),
    );
    let tool_roots = protectorate_sleeve_runner::default_tool_roots(&owner_home);
    assert!(tool_roots.contains(&owner_home.join(".local/go")));
    let socket = format!("p-rust-default-tools-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::new(Manager::new(&socket)))
        .with_terminal_launcher(TerminalLauncher {
            state_root: state,
            user_home: owner_home,
            runtime_dir: runtime,
            runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            hub_url: format!("http://{addr}"),
            cli_executable: None,
            tool_roots,
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            tasks_max: 64,
        });
    let (_, server) = serve(listener, hub).await;
    let (status, spawned) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({"name":"tools","harness":"HARNESS_SHELL","workspace":workspace,"args":["go"]}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let proof = workspace.join("proof");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !proof.is_file() {
        assert!(Instant::now() < deadline, "default Go tool did not run");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let output = fs::read_to_string(proof).unwrap();
    assert!(output.starts_with("go-visible:"), "{output}");
    assert!(output.contains(".local/go/bin"), "{output}");
    let (status, killed) = call(
        addr,
        "/protectorate.v1.SleeveService/KillSleeve",
        json!({"name":"tools"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(
    listener: TcpListener,
    hub: Hub,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}
async fn call(addr: std::net::SocketAddr, path: &str, body: Value) -> (u16, Value) {
    call_as(addr, path, "owner-secret", body).await
}
async fn call_as(addr: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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
async fn http_spawn_capture_and_kill_real_sandboxed_shell() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner_home = root.path().join("owner");
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    for dir in [&state, &owner_home, &workspace, &runtime] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(root.path().join("hidden-secret"), "should not appear").unwrap();
    let sdk = root.path().join("flutter");
    let sdk_cache = sdk.join("bin/cache");
    let lower = state.join("cache/flutter-test");
    let tools_cache = sdk.join("packages/flutter_tools/.dart_tool");
    let tools_lower = state.join("cache/flutter-tools-test");
    fs::create_dir_all(&sdk_cache).unwrap();
    fs::create_dir_all(&lower).unwrap();
    fs::create_dir_all(&tools_cache).unwrap();
    fs::create_dir_all(&tools_lower).unwrap();
    fs::write(sdk_cache.join("shared"), "host-original").unwrap();
    fs::write(lower.join("shared"), "published-original").unwrap();
    fs::write(tools_cache.join("config"), "host-tools").unwrap();
    fs::write(tools_lower.join("config"), "published-tools").unwrap();
    fs::write(
        lower.join(".protectorate-flutter-cache.json"),
        json!({"sdk":sdk,"sha256":"test","bytes":18,"entries":1}).to_string(),
    )
    .unwrap();
    fs::write(
        tools_lower.join(".protectorate-flutter-tools.json"),
        json!({"sdk":sdk,"sha256":"test"}).to_string(),
    )
    .unwrap();
    let fake_cli = root.path().join("protectorate");
    executable::write_executable(
        &fake_cli,
        "#!/usr/bin/sh\nif [ \"$1\" = mcp ]; then shift; exec /opt/protectorate/bin/protectorate-mcpbridge \"$@\"; fi\nprintf 'cli-ok:%s' \"$PROTECTORATE_SLEEVE\"\n",
    );
    let fake_bridge = root.path().join("protectorate-mcpbridge");
    executable::write_executable(
        &fake_bridge,
        "#!/usr/bin/sh\n[ \"$1\" = --list ] || exit 2\nprintf '{\"tools\":[]}'\n",
    );
    let real_mcp = std::env::var_os("PROTECTORATE_TEST_REAL_MCP").is_some();
    let cli = if real_mcp {
        let build_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let cli = build_dir.join("protectorate");
        assert!(
            cli.is_file(),
            "build protectorate before the live MCP trial"
        );
        assert!(
            build_dir.join("protectorate-mcpbridge").is_file(),
            "build protectorate-mcpbridge before the live MCP trial"
        );
        cli
    } else {
        fake_cli
    };
    let socket = format!("p-rust-sleeve-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_addr = listener.local_addr().unwrap();
    let launcher = TerminalLauncher {
        state_root: state.clone(),
        user_home: owner_home.clone(),
        runtime_dir: runtime.clone(),
        runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        hub_url: format!("http://{first_addr}"),
        cli_executable: Some(cli),
        tool_roots: vec![sdk.clone()],
        flutter_sdk_root: Some(sdk.clone()),
        flutter_cache_lower: Some(lower.clone()),
        flutter_tools_lower: Some(tools_lower.clone()),
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        tasks_max: 64,
    };
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager))
        .with_terminal_launcher(launcher.clone());
    let (addr, task) = serve(listener, hub).await;
    let (status, spawned) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({
            "name":"shell", "harness":"HARNESS_SHELL", "workspace":workspace,
        "env":{"PUB_CACHE":"/tmp/escaped-pub","GOCACHE":"/tmp/escaped-go"},
        "args":["sh"]
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["state"], "SLEEVE_STATE_RUNNING");
    assert_eq!(spawned["sleeve"]["session"], "sleeve-shell");
    let token_path = state.join("sleeves/shell/home/.protectorate-token");
    let token = fs::read_to_string(&token_path).unwrap();
    let token = token.trim();
    let (status, self_status) = call_as(
        addr,
        "/protectorate.v1.HubService/GetStatus",
        token,
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{self_status}");
    let (duplicate_status, duplicate) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({
            "name":"shell", "harness":"HARNESS_SHELL", "workspace":workspace
        }),
    )
    .await;
    assert_eq!(duplicate_status, 409, "{duplicate}");
    let (forbidden_status, forbidden) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({
            "name":"forbidden", "harness":"HARNESS_SHELL", "workspace":root.path().join("owner")
        }),
    )
    .await;
    assert_eq!(forbidden_status, 400, "{forbidden}");
    for (name, option, message) in [
        (
            "supervised-shell",
            json!({"supervised":true}),
            "supervised requires chat mode",
        ),
        (
            "brief-shell",
            json!({"brief":"hello"}),
            "brief requires chat mode",
        ),
    ] {
        let mut request = json!({"name":name,"harness":"HARNESS_SHELL","workspace":workspace});
        request
            .as_object_mut()
            .unwrap()
            .extend(option.as_object().unwrap().clone());
        let (status, response) =
            call(addr, "/protectorate.v1.SleeveService/SpawnSleeve", request).await;
        assert_eq!(status, 400, "{response}");
        assert_eq!(response["message"], message);
    }
    let cli_command = if real_mcp {
        "protectorate version > cli-proof"
    } else {
        "protectorate > cli-proof"
    };
    let shell_input = format!(
        "test -t 0 && printf tty-ok > tty-proof; test -s \"$PROTECTORATE_TOKEN_FILE\" && printf '%s|%s' \"$PROTECTORATE_HUB\" \"$PROTECTORATE_TOKEN_FILE\" > env-proof; printf '%s|%s|%s|%s' \"$PUB_CACHE\" \"$GOCACHE\" \"$GOMODCACHE\" \"$CARGO_HOME\" > cache-env-proof; printf pub-ok > \"$PUB_CACHE/probe\"; printf cargo-ok > \"$CARGO_HOME/probe\"; cat '{sdk}/bin/cache/shared' > flutter-lower-proof; printf private-copy > '{sdk}/bin/cache/shared'; cat '{sdk}/packages/flutter_tools/.dart_tool/config' > flutter-tools-proof; printf private-tools > '{sdk}/packages/flutter_tools/.dart_tool/config'; {cli_command}; protectorate mcp --list > mcp-proof; printf 'live-proof\\n'; printf sandbox-write > proof",
        sdk=sdk.display(),
    );
    let sent = std::process::Command::new("tmux")
        .args([
            "-L",
            &socket,
            "send-keys",
            "-t",
            "=sleeve-shell:",
            &shell_input,
            "Enter",
        ])
        .status()
        .unwrap();
    assert!(sent.success(), "could not send shell input to the tmux pty");
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let (status, captured) = call(
            addr,
            "/protectorate.v1.SessionService/CapturePane",
            json!({"name":"sleeve-shell","lines":40}),
        )
        .await;
        assert_eq!(status, 200, "{captured}");
        if captured["text"]
            .as_str()
            .unwrap_or_default()
            .contains("live-proof")
            && workspace.join("proof").is_file()
            && workspace.join("env-proof").is_file()
            && workspace.join("cache-env-proof").is_file()
            && workspace.join("cli-proof").is_file()
            && workspace.join("mcp-proof").is_file()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pane never printed proof: {captured}"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(
        fs::read_to_string(workspace.join("proof")).unwrap(),
        "sandbox-write"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("tty-proof")).unwrap(),
        "tty-ok"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("env-proof")).unwrap(),
        format!(
            "http://{addr}|{}",
            owner_home.join(".protectorate-token").display()
        )
    );
    let cli_proof = fs::read_to_string(workspace.join("cli-proof")).unwrap();
    if real_mcp {
        assert!(cli_proof.starts_with("protectorate "), "{cli_proof}");
        let catalog: Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("mcp-proof")).unwrap())
                .unwrap();
        assert!(catalog.as_array().is_some_and(|tools| !tools.is_empty()));
    } else {
        assert_eq!(cli_proof, "cli-ok:shell");
        assert_eq!(
            fs::read_to_string(workspace.join("mcp-proof")).unwrap(),
            "{\"tools\":[]}"
        );
    }
    assert_eq!(
        fs::read_to_string(workspace.join("cache-env-proof")).unwrap(),
        format!(
            "{}|{}|{}|{}",
            owner_home.join(".cache/pub").display(),
            owner_home.join(".cache/go-build").display(),
            owner_home.join(".cache/go-mod").display(),
            owner_home.join(".cargo").display(),
        )
    );
    assert_eq!(
        fs::read_to_string(state.join("sleeves/shell/home/.cache/pub/probe")).unwrap(),
        "pub-ok"
    );
    assert_eq!(
        fs::read_to_string(state.join("sleeves/shell/home/.cargo/probe")).unwrap(),
        "cargo-ok"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("flutter-lower-proof")).unwrap(),
        "published-original"
    );
    assert_eq!(
        fs::read_to_string(sdk_cache.join("shared")).unwrap(),
        "host-original"
    );
    assert_eq!(
        fs::read_to_string(lower.join("shared")).unwrap(),
        "published-original"
    );
    assert_eq!(
        fs::read_to_string(state.join("sleeves/shell/overlays/flutter/upper/shared")).unwrap(),
        "private-copy"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("flutter-tools-proof")).unwrap(),
        "published-tools"
    );
    assert_eq!(
        fs::read_to_string(tools_cache.join("config")).unwrap(),
        "host-tools"
    );
    assert_eq!(
        fs::read_to_string(tools_lower.join("config")).unwrap(),
        "published-tools"
    );
    assert_eq!(
        fs::read_to_string(state.join("sleeves/shell/overlays/flutter-tools/upper/config"))
            .unwrap(),
        "private-tools"
    );
    task.abort();
    let _ = task.await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut launcher = launcher;
    launcher.hub_url = format!("http://{}", listener.local_addr().unwrap());
    let restarted = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager))
        .with_terminal_launcher(launcher);
    let (addr, task) = serve(listener, restarted).await;
    let (status, live) = call(
        addr,
        "/protectorate.v1.SleeveService/GetSleeve",
        json!({"name":"shell"}),
    )
    .await;
    assert_eq!(status, 200, "{live}");
    assert_eq!(live["sleeve"]["state"], "SLEEVE_STATE_RUNNING");
    let cache = state.join("sleeves/shell/home/.cache/pub");
    let overlay = state.join("sleeves/shell/overlays/pub/upper");
    fs::create_dir_all(&cache).unwrap();
    fs::create_dir_all(&overlay).unwrap();
    fs::write(cache.join("artifact"), "throw away").unwrap();
    fs::write(overlay.join("artifact"), "throw away").unwrap();
    let kept_home_file = state.join("sleeves/shell/home/keep.txt");
    fs::write(&kept_home_file, "keep").unwrap();
    let (status, killed) = call(
        addr,
        "/protectorate.v1.SleeveService/KillSleeve",
        json!({"name":"shell","dropCaches":true}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    let (status, row) = call(
        addr,
        "/protectorate.v1.SleeveService/GetSleeve",
        json!({"name":"shell"}),
    )
    .await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["sleeve"]["state"], "SLEEVE_STATE_EXITED");
    assert_eq!(row["sleeve"]["killed"], true);
    assert!(!cache.exists());
    assert!(!state.join("sleeves/shell/overlays").exists());
    assert_eq!(fs::read_to_string(kept_home_file).unwrap(), "keep");
    assert!(!token_path.exists());
    let (status, denied) = call_as(
        addr,
        "/protectorate.v1.HubService/GetStatus",
        token,
        json!({}),
    )
    .await;
    assert_eq!(status, 401, "{denied}");

    let (status, spawned) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({"name":"natural","harness":"HARNESS_SHELL","workspace":workspace,"args":["sh"]}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let natural_token_path = state.join("sleeves/natural/home/.protectorate-token");
    let natural_token = fs::read_to_string(&natural_token_path).unwrap();
    let exited = std::process::Command::new("tmux")
        .args([
            "-L",
            &socket,
            "send-keys",
            "-t",
            "=sleeve-natural:",
            "exit",
            "Enter",
        ])
        .status()
        .unwrap();
    assert!(exited.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    while natural_token_path.exists() {
        assert!(
            Instant::now() < deadline,
            "natural exit left an active sleeve token"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (status, denied) = call_as(
        addr,
        "/protectorate.v1.HubService/GetStatus",
        natural_token.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 401, "{denied}");
    task.abort();
}

// Run with PROTECTORATE_TEST_PUB_LOWER set to an explicitly published cache.
// This exercises hub HTTP launch wiring, not just the sleeve runner in isolation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn http_terminal_sleeve_resolves_published_pub_cache_offline() {
    let lower = std::path::PathBuf::from(std::env::var("PROTECTORATE_TEST_PUB_LOWER").unwrap());
    let owner_home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
    let dart_sdk = std::env::var_os("PROTECTORATE_TEST_DART_SDK")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| owner_home.join("development/flutter/bin/cache/dart-sdk"));
    assert!(dart_sdk.join("bin/dart").is_file());
    let root = tempfile::tempdir().unwrap();
    let state = lower.parent().unwrap().parent().unwrap().to_path_buf();
    let name = format!("pub-hub-{}", std::process::id());
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    for dir in [&workspace, &runtime] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        workspace.join("pubspec.yaml"),
        "name: protectorate_hub_pub_probe\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  collection: 1.19.1\n",
    )
    .unwrap();
    let socket = format!("p-rust-hub-pub-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_sessions(manager)
        .with_terminal_launcher(TerminalLauncher {
            state_root: state.clone(),
            user_home: owner_home.clone(),
            runtime_dir: runtime,
            runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            hub_url: format!("http://{addr}"),
            cli_executable: None,
            tool_roots: vec![dart_sdk],
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: Some(lower.clone()),
            go_mod_cache_lower: None,
            tasks_max: 64,
        });
    let (_, server) = serve(listener, hub).await;
    let (status, spawned) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({"name":name,"harness":"HARNESS_SHELL","workspace":workspace,"args":["sh"]}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let marker = format!("protectorate-hub-private-proof-{}", std::process::id());
    let owner_marker = owner_home.join(".pub-cache/hosted/pub.dev").join(&marker);
    let lower_marker = lower.join("hosted/pub.dev").join(&marker);
    assert!(!owner_marker.exists());
    assert!(!lower_marker.exists());
    let shell_input = format!(
        "set -eu; test \"$PUB_CACHE\" = \"$HOME/.pub-cache\"; command -v dart; command -v protoc-gen-dart; protoc-gen-dart </dev/null > plugin-response.bin; test -s plugin-response.bin; dart pub get --offline > pub-output 2>&1; test -f .dart_tool/package_config.json; printf private > \"$PUB_CACHE/hosted/pub.dev/{marker}\"; printf passed > pub-proof"
    );
    let session = format!("=sleeve-{name}:");
    let sent = std::process::Command::new("tmux")
        .args([
            "-L",
            &socket,
            "send-keys",
            "-t",
            &session,
            &shell_input,
            "Enter",
        ])
        .status()
        .unwrap();
    assert!(sent.success());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !workspace.join("pub-proof").is_file() {
        assert!(
            Instant::now() < deadline,
            "hub terminal sleeve did not finish Pub probe: {}",
            fs::read_to_string(workspace.join("pub-output")).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        workspace
            .join("plugin-response.bin")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    assert!(workspace.join(".dart_tool/package_config.json").is_file());
    assert_eq!(
        fs::read_to_string(workspace.join("pub-proof")).unwrap(),
        "passed"
    );
    assert_eq!(
        fs::read_to_string(
            state
                .join("sleeves")
                .join(&name)
                .join("overlays/pub/upper/hosted/pub.dev")
                .join(&marker)
        )
        .unwrap(),
        "private"
    );
    assert!(!owner_marker.exists());
    assert!(!lower_marker.exists());
    let (status, killed) = call(
        addr,
        "/protectorate.v1.SleeveService/KillSleeve",
        json!({"name":name}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_chat_sleeve_mounts_published_pub_cache_privately() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner_home = root.path().join("owner");
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    let tools = root.path().join("tools");
    let lower = state.join("cache/pub-test");
    for dir in [
        &state,
        &owner_home,
        &workspace,
        &runtime,
        &tools,
        &state.join("auth/pi"),
        &lower.join("hosted/pub.dev"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        lower.join(".protectorate-pub-cache.json"),
        json!({"source":owner_home.join(".pub-cache"),"sha256":"test"}).to_string(),
    )
    .unwrap();
    fs::write(lower.join("hosted/pub.dev/published.txt"), "published").unwrap();
    let pi = tools.join("pi");
    executable::write_executable(
        &pi,
        r#"#!/usr/bin/python3
import json, os, sys
for line in sys.stdin:
    message = json.loads(line)
    if message.get('type') == 'get_state':
        cache = os.environ['PUB_CACHE']
        assert cache == os.path.join(os.environ['HOME'], '.pub-cache')
        with open(os.path.join(cache, 'hosted/pub.dev/published.txt')) as source:
            assert source.read() == 'published'
        with open(os.path.join(cache, 'hosted/pub.dev/chat-private.txt'), 'w') as target:
            target.write('private')
        with open('chat-pub-proof', 'w') as proof:
            proof.write(cache)
        data = {'sessionId':'pi-pub-session','model':{'id':'test','provider':'local','contextWindow':1024},'thinkingLevel':'medium'}
        print(json.dumps({'type':'response','id':message['id'],'command':'get_state','success':True,'data':data}), flush=True)
    elif message.get('type') == 'get_available_models':
        data = {'models':[{'id':'test','provider':'local','name':'Test','reasoning':True}]}
        print(json.dumps({'type':'response','id':message['id'],'command':'get_available_models','success':True,'data':data}), flush=True)
    elif message.get('type') == 'get_available_thinking_levels':
        print(json.dumps({'type':'response','id':message['id'],'command':'get_available_thinking_levels','success':True,'data':{'levels':['off','low','medium','high']}}), flush=True)
    elif message.get('type') == 'set_model':
        data = {'id':message['modelId'],'provider':message['provider']}
        print(json.dumps({'type':'response','id':message['id'],'command':'set_model','success':True,'data':data}), flush=True)
    elif message.get('type') == 'set_thinking_level':
        print(json.dumps({'type':'response','id':message['id'],'command':'set_thinking_level','success':True}), flush=True)
"#,
    );
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner_home.clone(),
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        claude_tool_roots: Vec::new(),
        codex_executable: None,
        codex_tool_roots: Vec::new(),
        grok_executable: None,
        grok_tool_roots: Vec::new(),
        tool_roots: vec![tools],
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: Some(lower.clone()),
        go_mod_cache_lower: None,
        environment: BTreeMap::new(),
        tasks_max: 64,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    let (_, server) = serve(listener, hub).await;
    let (status, spawned) = call(
        addr,
        "/protectorate.v1.SleeveService/SpawnSleeve",
        json!({"name":"pi-pub","harness":"HARNESS_PI","workspace":workspace}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(
        spawned["sleeve"]["chat"]["harnessSessionId"],
        "pi-pub-session"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("chat-pub-proof")).unwrap(),
        owner_home.join(".pub-cache").to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(
            state.join("sleeves/pi-pub/overlays/pub/upper/hosted/pub.dev/chat-private.txt")
        )
        .unwrap(),
        "private"
    );
    assert!(!lower.join("hosted/pub.dev/chat-private.txt").exists());
    assert!(!owner_home
        .join(".pub-cache/hosted/pub.dev/chat-private.txt")
        .exists());
    let (status, killed) = call(
        addr,
        "/protectorate.v1.SleeveService/KillSleeve",
        json!({"name":"pi-pub"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}
