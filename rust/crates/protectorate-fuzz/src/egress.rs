//! A sandbox reaches the network only through its CONNECT proxy, so the
//! allowlist and the request head it parses decide every way out.
use crate::{fields, text};
use protectorate_sandbox::{
    egress::{Decision, Proxy, Verdict},
    Egress, EgressPolicy, Limits, Network, Policy, Request, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{IpAddr, Shutdown, TcpListener},
    os::unix::net::UnixStream,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

fn policy(root: &Path, egress: Egress) -> Option<EgressPolicy> {
    let state = root.join("state");
    let owner = root.join("owner");
    let workspace = root.join("project");
    let home = state.join("sleeves/net/home");
    for dir in [&state, &owner, &workspace, &home] {
        fs::create_dir_all(dir).unwrap();
    }
    let plan = Policy {
        state_root: state,
        user_home: owner,
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    }
    .plan(Request {
        workload: Workload::ChatSleeve,
        command: vec!["/bin/sh".into()],
        workspace: Some(workspace),
        managed_workspace_name: None,
        private_home: Some(home),
        grants: Vec::new(),
        tool_roots: Vec::new(),
        selected_executable: None,
        credential: None,
        overlays: Vec::new(),
        masks: Vec::new(),
        limits: Limits {
            cpu_percent: 100,
            memory_bytes: 256 << 20,
            tasks_max: 64,
        },
        egress: Some(egress),
    })
    .ok()?;
    match plan.network() {
        Network::Proxied(policy) => Some(policy.clone()),
        other => panic!("egress plan without a proxy: {other:?}"),
    }
}

fn lines(bytes: &[u8]) -> Vec<String> {
    text(bytes).lines().map(str::to_owned).collect()
}

/// Case and trailing dots never change a verdict, and IP literals pass only
/// the hub or a private-host rule.
fn check_admission(policy: &EgressPolicy, target: &str) {
    let Some((host, port)) = target
        .rsplit_once(':')
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
    else {
        return;
    };
    let verdict = policy.admit(host, port);
    assert_eq!(verdict, policy.admit(&host.to_ascii_uppercase(), port));
    assert_eq!(verdict, policy.admit(&format!("{host}."), port));
    let Ok(admission) = verdict else {
        return;
    };
    let bare = host.trim_end_matches('.');
    let bare = bare
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(bare);
    if bare.parse::<IpAddr>().is_ok() {
        assert!(
            matches!(admission.rule, "hub" | "local-model"),
            "IP literal {host} admitted by the {} rule",
            admission.rule
        );
    }
    if admission.rule == "operator" {
        assert!(!admission.private, "operator rules reach private addresses");
    }
}

/// The proxy's only admissible destination is a loopback hub that accepts
/// and hangs up, so no input reaches another service or the network. Each
/// `$` in the head stands for the hub's port.
fn check_connect(root: &Path, head: &[u8]) {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = upstream.local_addr().unwrap().port();
    let Some(policy) = policy(
        root,
        Egress {
            hub: Some(format!("127.0.0.1:{port}")),
            allow: Vec::new(),
            private_hosts: Vec::new(),
        },
    ) else {
        panic!("the fixed loopback hub policy was refused");
    };
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    let decisions = Arc::new(Mutex::new(Vec::<Decision>::new()));
    let sink = Arc::clone(&decisions);
    let proxy = Proxy::start(
        &policy,
        &runtime,
        Some(Arc::new(move |decision: &Decision| {
            sink.lock().unwrap().push(decision.clone())
        })),
    )
    .unwrap();
    upstream.set_nonblocking(true).unwrap();
    let mut stream = UnixStream::connect(&proxy.sockets[0].0).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let head = head[..head.len().min(16 << 10)]
        .split(|&b| b == b'$')
        .map(<[u8]>::to_vec)
        .collect::<Vec<_>>()
        .join(port.to_string().as_bytes());
    let _ = stream.write_all(&head);
    let _ = stream.shutdown(Shutdown::Write);
    let mut response = Vec::new();
    let reader = thread::spawn(move || {
        let _ = stream.read_to_end(&mut response);
        response
    });
    let response = loop {
        if let Ok((tunnel, _)) = upstream.accept() {
            drop(tunnel);
        }
        if reader.is_finished() {
            break reader.join().unwrap();
        }
        thread::sleep(Duration::from_millis(1));
    };
    drop(proxy);
    let status = String::from_utf8_lossy(&response)
        .split(' ')
        .nth(1)
        .unwrap_or("")
        .to_owned();
    let decisions = decisions.lock().unwrap().clone();
    assert!(decisions.len() <= 1, "{decisions:?}");
    let Some(decision) = decisions.first() else {
        assert!(
            status.is_empty() || status == "431",
            "{status} without a decision"
        );
        return;
    };
    match decision.verdict {
        Verdict::Allow => {
            assert_eq!(status, "200", "{decision:?}");
            assert!(policy.admit(&decision.host, decision.port).is_ok());
            let address: IpAddr = decision
                .reason
                .rsplit("; ")
                .next()
                .and_then(|a| a.parse().ok())
                .expect("an allowed tunnel names its address");
            assert!(address.is_loopback(), "{decision:?}");
        }
        Verdict::Deny => assert!(
            ["400", "403", "405"].contains(&status.as_str()),
            "{status} for {decision:?}"
        ),
        Verdict::Error => assert_eq!(status, "502", "{decision:?}"),
    }
}

/// Allowlist, private hosts and hub spec, CONNECT targets to judge against
/// them, then one raw request head sent to a running proxy.
pub fn egress_connect(data: &[u8]) {
    let parts = fields(data, 5);
    let root = tempfile::tempdir().unwrap();
    let egress = Egress {
        hub: Some(text(parts[2]).trim().to_owned()).filter(|hub| !hub.is_empty()),
        allow: lines(parts[0]),
        private_hosts: lines(parts[1]),
    };
    if let Some(policy) = policy(root.path(), egress) {
        for target in lines(parts[3]) {
            check_admission(&policy, &target);
        }
    }
    check_connect(root.path(), parts[4]);
}
