#![cfg(unix)]

use protectorate_cli::{
    client_platform, client_version, dispatch_endpoint, parse, version_text, Code, Command,
};
use protectorate_hub::Hub;
use serde_json::Value;
use tokio::net::TcpListener;

#[test]
fn version_reports_the_client_build_without_touching_the_hub() {
    let text = version_text(false);
    assert_eq!(
        text,
        format!(
            "protectorate-cli {} {}\n",
            client_version(),
            client_platform()
        )
    );
    // No endpoint, no token: version never opens a connection.
    let text = dispatch_endpoint("http://127.0.0.1:1", "", ["version"]).unwrap();
    assert_eq!(text, version_text(false));
    assert_eq!(
        protectorate_cli::run(["version"]).unwrap(),
        version_text(false)
    );
    let json = dispatch_endpoint("http://127.0.0.1:1", "", ["--json", "version"]).unwrap();
    let reported: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json.lines().count(), 1, "{json}");
    assert_eq!(reported["version"], client_version());
    assert_eq!(
        format!(
            "{}/{}",
            reported["os"].as_str().unwrap(),
            reported["arch"].as_str().unwrap()
        ),
        client_platform()
    );
}

#[test]
fn api_call_parses_procedure_and_body() {
    struct Case {
        args: &'static [&'static str],
        procedure: &'static str,
        body: &'static str,
        json: bool,
    }
    let cases = [
        Case {
            args: &["api", "call", "HubService/GetStatus"],
            procedure: "/protectorate.v1.HubService/GetStatus",
            body: "{}",
            json: false,
        },
        Case {
            args: &["api", "call", "HubService/GetStatus", "{}"],
            procedure: "/protectorate.v1.HubService/GetStatus",
            body: "{}",
            json: false,
        },
        Case {
            args: &["api", "call", "QuotaService/GetQuota", "{\"refresh\":true}"],
            procedure: "/protectorate.v1.QuotaService/GetQuota",
            body: "{\"refresh\":true}",
            json: false,
        },
        Case {
            args: &["api", "call", "protectorate.v1.HubService/GetStatus"],
            procedure: "/protectorate.v1.HubService/GetStatus",
            body: "{}",
            json: false,
        },
        Case {
            args: &["--json", "api", "call", "NodeService/ListNodes"],
            procedure: "/protectorate.v1.NodeService/ListNodes",
            body: "{}",
            json: true,
        },
        Case {
            args: &["api", "call", "HubService/GetStatus", "--json"],
            procedure: "/protectorate.v1.HubService/GetStatus",
            body: "{}",
            json: true,
        },
    ];
    for case in cases {
        let invocation =
            parse(case.args).unwrap_or_else(|error| panic!("{:?}: {error}", case.args));
        let Command::ApiCall { procedure, body } = invocation.command else {
            panic!("{:?}: not an api call", case.args)
        };
        assert_eq!(procedure, case.procedure, "{:?}", case.args);
        assert_eq!(body, case.body, "{:?}", case.args);
        assert_eq!(invocation.json, case.json, "{:?}", case.args);
    }
    let invocation = parse([
        "api",
        "call",
        "HubService/GetStatus",
        "--hub",
        "https://hub.example",
        "--token-file",
        "/tmp/token",
    ])
    .unwrap();
    assert_eq!(invocation.hub.as_deref(), Some("https://hub.example"));
    assert_eq!(
        invocation.token_file.as_deref(),
        Some(std::path::Path::new("/tmp/token"))
    );
}

#[test]
fn bad_api_call_and_version_lines_are_invalid_argument() {
    let cases = [
        vec!["api"],
        vec!["api", "bogus"],
        vec!["api", "call"],
        vec!["api", "call", "HubService"],
        vec!["api", "call", "/GetStatus"],
        vec!["api", "call", "HubService/"],
        vec!["api", "call", "HubService/Get/Status"],
        vec!["api", "call", "Hub Service/GetStatus"],
        vec!["api", "call", "1HubService/GetStatus"],
        vec!["api", "call", "HubService/1GetStatus"],
        vec!["api", "call", "Hub/../Service/GetStatus"],
        vec!["api", "call", "HubService/GetStatus", "not json"],
        vec!["api", "call", "HubService/GetStatus", "{", "}"],
        vec!["api", "call", "HubService/GetStatus", "{}", "extra"],
        vec!["api", "call", "HubService/GetStatus", "--token-file"],
        vec!["api", "call", "HubService/GetStatus", "--typo"],
        vec!["api", "call", "HubService/GetStatus", "version"],
        vec!["version", "extra"],
        vec!["version", "api", "call", "HubService/GetStatus"],
    ];
    for args in cases {
        let error = parse(&args).unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{args:?}");
        assert!(error.message.contains("Usage:"), "{args:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn api_call_runs_over_real_hub_http() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| dispatch_endpoint(&url, "owner-secret", args);

        let pretty = call(&["api", "call", "HubService/GetStatus"]).unwrap();
        let status: Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(status["version"], "test", "{pretty}");
        assert!(pretty.starts_with("{\n  \""), "{pretty}");
        assert_eq!(
            call(&["api", "call", "HubService/GetStatus", "{}"]).unwrap(),
            pretty
        );

        let compact = call(&["--json", "api", "call", "HubService/GetStatus"]).unwrap();
        let same: Value = serde_json::from_str(&compact).unwrap();
        assert_eq!(compact.lines().count(), 1, "{compact}");
        assert_eq!(same, status);

        // A procedure the dispatcher has no command for is still reachable.
        let nodes = call(&["api", "call", "NodeService/ListNodes", "{}"]).unwrap();
        assert!(nodes.contains("\"nodes\""), "{nodes}");
        let quota = call(&["api", "call", "QuotaService/GetQuota", "{\"refresh\":true}"]).unwrap();
        let quota: Value = serde_json::from_str(&quota).unwrap();
        assert!(quota["quotas"].is_array(), "{quota}");

        let error = call(&["api", "call", "HubService/NotAProcedure"]).unwrap_err();
        assert_eq!(error.code, Code::Unimplemented, "{error:?}");
        let error = call(&["api", "call", "EventService/Subscribe", "{}"]).unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{error:?}");
        let error = call(&["api", "call", "HubService/GetStatus", "{\"bogus\":1}"]).unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{error:?}");
        let error =
            dispatch_endpoint(&url, "", ["api", "call", "HubService/GetStatus"]).unwrap_err();
        assert_eq!(error.code, Code::PermissionDenied, "{error:?}");
    })
    .await;
    server.abort();
    result.unwrap();
}
