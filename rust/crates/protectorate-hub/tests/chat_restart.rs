use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record, State};
use std::{fs, time::Duration};

#[test]
fn restart_revokes_tokens_for_chat_processes_it_cannot_reattach() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let workspace = home.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let hub = Hub::open(home, "owner-secret", "test").unwrap();
    let record = Record::new(
        "stale",
        Harness::Pi,
        Mode::Chat,
        &workspace,
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    hub.sleeves.record_spawned(record).unwrap();
    let now = chrono::Utc::now().timestamp();
    let issued = hub
        .identity
        .create("sleeve:stale", &["read"], Duration::ZERO, now, &[])
        .unwrap();
    let token_file = home.join("sleeves/stale/home/.protectorate-token");
    fs::create_dir_all(token_file.parent().unwrap()).unwrap();
    fs::write(&token_file, issued.secret.expose()).unwrap();
    assert!(hub.identity.lookup(issued.secret.expose(), now).is_ok());
    drop(hub);

    let restarted = Hub::open(home, "owner-secret", "test").unwrap();
    assert_eq!(restarted.sleeves.get("stale").unwrap().state, State::Exited);
    assert!(restarted
        .identity
        .lookup(issued.secret.expose(), now)
        .is_err());
    assert!(!token_file.exists());
}
