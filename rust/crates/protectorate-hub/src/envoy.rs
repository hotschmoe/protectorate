use crate::{sleeves, Hub};
use protectorate_identity::Token;
use protectorate_sleeve_registry::State;
use protectorate_wire::{Catalog, Code, Result, RpcError};
use std::{fs, path::Path, sync::Arc, time::Duration};

const WATCH_EVERY: Duration = Duration::from_secs(60);
const TOKEN_GRACE: i64 = 5 * 60;
const TOKEN_NAME: &str = "orchestrator-envoy";

fn plan_renewal(tokens: &[Token], now: i64) -> (Option<Token>, Vec<Token>) {
    let mut live = tokens
        .iter()
        .filter(|token| token.name == TOKEN_NAME && token.revoked_at.is_none())
        .cloned()
        .collect::<Vec<_>>();
    live.sort_by_key(|token| token.created);
    let Some(newest) = live.pop() else {
        return (None, Vec::new());
    };
    let revoke = if now >= newest.created.saturating_add(TOKEN_GRACE) {
        live
    } else {
        Vec::new()
    };
    let rotate = newest.expires.and_then(|expires| {
        let life = expires.saturating_sub(newest.created);
        (life > 0 && expires.saturating_sub(now) <= life / 4).then_some(newest)
    });
    (rotate, revoke)
}

pub(super) fn renew(hub: &Hub, now: i64) -> Result<()> {
    let _guard = hub.envoy_spawn_lock.lock().unwrap();
    let (rotate, revoke) = plan_renewal(&hub.identity.list(), now);
    for token in revoke {
        hub.identity
            .revoke(&token.id, now)
            .map_err(super::token_error)?;
    }
    let Ok(row) = hub.sleeves.get("envoy") else {
        return Ok(());
    };
    if row.state != State::Running {
        return Ok(());
    }
    let workspace = hub.workspaces.state_root().join("workspaces/envoy");
    if Path::new(&row.workspace) != workspace
        || fs::canonicalize(&workspace).ok().as_deref() != Some(workspace.as_path())
    {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "Envoy workspace is not the managed directory",
        ));
    }
    if let Some(old) = rotate {
        let expires = old.expires.expect("planned rotation has expiry");
        let life = u64::try_from(expires.saturating_sub(old.created))
            .map_err(|_| RpcError::new(Code::Internal, "Envoy token has an invalid lifetime"))?;
        let ttl = Duration::from_secs(life);
        let issued = if old.is_active(now) {
            hub.identity
                .renew(&old.id, ttl, now)
                .map_err(super::token_create_error)?
        } else {
            let catalog = Catalog::embedded()?;
            let methods = catalog.methods().cloned().collect::<Vec<_>>();
            let scope_methods = super::scope_methods(&methods);
            hub.identity
                .create(
                    TOKEN_NAME,
                    &old.scopes.iter().map(String::as_str).collect::<Vec<_>>(),
                    ttl,
                    now,
                    &scope_methods,
                )
                .map_err(super::token_create_error)?
        };
        let token_path = workspace.join(".protectorate-token");
        if let Err(error) = sleeves::envoy_write_if_changed(
            &token_path,
            format!("{}\n", issued.secret.expose()).as_bytes(),
            0o600,
        ) {
            let _ = hub.identity.revoke(&issued.token.id, now);
            return Err(error);
        }
    }
    let hosts =
        fs::read_to_string(hub.workspaces.state_root().join("hosts.md")).unwrap_or_else(|_| {
            "# Hosts\n\nThe hub has not written an inventory yet. Run `protectorate node ls`.\n"
                .into()
        });
    sleeves::envoy_write_if_changed(&workspace.join("hosts.md"), hosts.as_bytes(), 0o644)
}

pub(super) fn start_watcher(runtime: &tokio::runtime::Handle, hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    runtime.spawn(async move {
        let mut interval = tokio::time::interval(WATCH_EVERY);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(hub) = weak.upgrade() else { break };
            match tokio::task::spawn_blocking(move || renew(&hub, super::now_unix())).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("Envoy renewal: {error}"),
                Err(error) => {
                    eprintln!("Envoy watcher stopped: {error}");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_identity::Store;
    use protectorate_sleeve_registry::{Harness, Mode, Record};
    use std::io::Read;

    #[test]
    fn rotation_keeps_old_token_for_five_minutes_and_preserves_lifetime() {
        let store = Store::memory();
        let now = 1_000;
        let first = store
            .create(
                "orchestrator-envoy",
                &["read"],
                Duration::from_secs(800),
                now,
                &[],
            )
            .unwrap();
        assert!(plan_renewal(&store.list(), now + 599).0.is_none());
        let (rotate, revoke) = plan_renewal(&store.list(), now + 600);
        assert_eq!(rotate.unwrap().id, first.token.id);
        assert!(revoke.is_empty());
        let second = store
            .renew(&first.token.id, Duration::from_secs(800), now + 600)
            .unwrap();
        let (rotate, revoke) = plan_renewal(&store.list(), now + 600 + TOKEN_GRACE - 1);
        assert!(rotate.is_none());
        assert!(revoke.is_empty());
        let (rotate, revoke) = plan_renewal(&store.list(), now + 600 + TOKEN_GRACE);
        assert!(rotate.is_none());
        assert_eq!(
            revoke.iter().map(|t| &t.id).collect::<Vec<_>>(),
            [&first.token.id]
        );
        assert_eq!(second.token.expires.unwrap() - second.token.created, 800);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn renewal_swaps_token_file_and_refreshes_hosts() {
        let root = tempfile::Builder::new()
            .prefix("protectorate-envoy-renewal-")
            .tempdir_in("/dev/shm")
            .unwrap()
            .keep();
        let hub = Hub::open(&root, "owner-secret", "test").unwrap();
        let workspace = root.join("workspaces/envoy");
        fs::create_dir_all(&workspace).unwrap();
        let row = Record::new("envoy", Harness::Pi, Mode::Chat, &workspace, "now").unwrap();
        hub.sleeves.record_spawned(row).unwrap();
        let catalog = Catalog::embedded().unwrap();
        let methods = catalog.methods().cloned().collect::<Vec<_>>();
        let scope_methods = super::super::scope_methods(&methods);
        let first = hub
            .identity
            .create(
                TOKEN_NAME,
                &["read"],
                Duration::from_secs(2_400),
                1_000,
                &scope_methods,
            )
            .unwrap();
        let token_path = workspace.join(".protectorate-token");
        sleeves::envoy_write_if_changed(
            &token_path,
            format!("{}\n", first.secret.expose()).as_bytes(),
            0o600,
        )
        .unwrap();
        let mut old_file = fs::File::open(&token_path).unwrap();
        fs::write(root.join("hosts.md"), "# Hosts\nnew inventory\n").unwrap();
        renew(&hub, 2_800).unwrap();
        let current_secret = fs::read_to_string(&token_path).unwrap();
        assert_ne!(current_secret.trim(), first.secret.expose());
        assert_eq!(
            fs::read_to_string(workspace.join("hosts.md")).unwrap(),
            "# Hosts\nnew inventory\n"
        );
        assert_eq!(
            hub.identity
                .lookup(first.secret.expose(), 3_099)
                .unwrap()
                .id,
            first.token.id
        );
        assert_eq!(
            hub.identity
                .lookup(current_secret.trim(), 3_099)
                .unwrap()
                .name,
            TOKEN_NAME
        );
        let mut old_contents = String::new();
        old_file.read_to_string(&mut old_contents).unwrap();
        assert_eq!(old_contents, format!("{}\n", first.secret.expose()));
        renew(&hub, 3_100).unwrap();
        assert!(hub
            .identity
            .list()
            .iter()
            .find(|t| t.id == first.token.id)
            .unwrap()
            .revoked_at
            .is_some());
    }

    #[test]
    fn renewal_ignores_exited_envoy_from_a_restored_hub() {
        let root = tempfile::Builder::new()
            .prefix("protectorate-envoy-exited-")
            .tempdir_in("/dev/shm")
            .unwrap()
            .keep();
        let hub = Hub::open(&root, "owner-secret", "test").unwrap();
        let old_workspace = root.join("old-home/workspaces/envoy");
        let row = Record::new("envoy", Harness::Pi, Mode::Chat, &old_workspace, "now").unwrap();
        hub.sleeves.record_spawned(row).unwrap();
        hub.sleeves
            .record_exit("envoy", "later".into(), None, 0, false, true)
            .unwrap();
        renew(&hub, 1_000).unwrap();
    }
}
