#![cfg(windows)]

#[test]
fn native_state_probe_is_read_only() {
    let home = tempfile::tempdir().unwrap();
    let controller = protectorate_desk_node::Controller::open(home.path()).unwrap();
    let state = controller.inspect_state().unwrap();
    assert!(state["sessionReady"].is_boolean());
    assert!(matches!(
        state["screenState"].as_str(),
        Some("locked" | "unlocked")
    ));
    assert!(state["available"].is_boolean());
    assert!(state["reason"].is_string());
    assert!(state["detail"].is_string());
    assert!(state["evaluatedAt"].is_string());
    assert!(!home.path().join("desk-policy.json").exists());
    println!(
        "session_ready={} screen_state={} reason={}",
        state["sessionReady"], state["screenState"], state["reason"]
    );
}

#[test]
#[ignore = "requires a logged-in Windows console; run only in an isolated interactive task"]
fn native_console_read_only_actions_and_policy() {
    use chrono::{Timelike, Utc};
    use serde_json::json;

    let home = tempfile::tempdir().unwrap();
    let controller = protectorate_desk_node::Controller::open(home.path()).unwrap();
    let initial = controller.inspect_state().unwrap();
    assert_eq!(initial["sessionReady"], true, "{initial}");
    assert_eq!(initial["screenState"], "unlocked", "{initial}");
    assert_eq!(initial["available"], true, "{initial}");

    let now = Utc::now();
    let start = (now - chrono::Duration::hours(1)).time();
    let end = (now + chrono::Duration::hours(1)).time();
    let mut policy = json!({
        "window": format!("{:02}:{:02}-{:02}:{:02}", start.hour(), start.minute(), end.hour(), end.minute()),
        "idle": "86400s",
        "timezone": "UTC",
        "lockAfter": false,
    });
    let state = controller.get_state(&json!({"policy":policy})).unwrap();
    assert_eq!(state["policy"], policy);
    assert!(state["lastInput"].is_string(), "{state}");
    assert_eq!(state["reason"], "DESK_REASON_USER_ACTIVE", "{state}");
    let denied = controller
        .action(&json!({"move":{"x":0,"y":0}}))
        .unwrap_err();
    assert_eq!(denied.kind, protectorate_desk_node::ErrorKind::Unavailable);

    policy["idle"] = json!("1s");
    assert_eq!(
        controller.get_state(&json!({"policy":policy})).unwrap()["policy"],
        policy
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    let state = controller.inspect_state().unwrap();
    assert_eq!(state["available"], true, "{state}");

    let screenshot = controller
        .action(&json!({"screenshot":{"maxWidth":320}}))
        .unwrap();
    let image = &screenshot["screenshot"];
    let (width, height) = (
        image["width"].as_i64().unwrap(),
        image["height"].as_i64().unwrap(),
    );
    assert!(width > 0 && height > 0);
    let png = image["png"].as_str().unwrap();
    assert!(png.len() > 100 && png.starts_with("iVBORw0KGgo"));

    let windows = controller.action(&json!({"listWindows":{}})).unwrap();
    assert!(windows["listWindows"]["windows"].is_array());
    let foreground = controller.action(&json!({"foreground":{}})).unwrap();
    assert!(foreground["foreground"]["agentIntegrity"].is_string());
    println!(
        "console desk probe: {width}x{height}, windows={}, policy reason={}, foreground blocks={}",
        windows["listWindows"]["windows"].as_array().unwrap().len(),
        state["reason"],
        foreground["foreground"]["blocks"]
    );
}
