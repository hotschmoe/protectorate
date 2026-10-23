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
                .map_err(super::token_error)?
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
                .map_err(super::token_error)?
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
