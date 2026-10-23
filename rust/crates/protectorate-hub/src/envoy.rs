use crate::{sleeves, Hub};
use protectorate_sleeve_registry::State;
use protectorate_wire::{Code, Result, RpcError};
use std::{fs, path::Path};

/// Keeps a running Envoy's `hosts.md` in step with the hub's inventory.
pub(super) fn write_hosts(hub: &Hub) -> Result<()> {
    let _guard = hub.envoy_spawn_lock.lock().unwrap();
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
    let hosts =
        fs::read_to_string(hub.workspaces.state_root().join("hosts.md")).unwrap_or_else(|_| {
            "# Hosts\n\nThe hub has not written an inventory yet. Run `protectorate node ls`.\n"
                .into()
        });
    sleeves::envoy_write_if_changed(&workspace.join("hosts.md"), hosts.as_bytes(), 0o644)
}
