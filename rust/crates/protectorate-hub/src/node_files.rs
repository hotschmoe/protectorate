//! Relay node file transfers without staging their contents on the hub.
use super::Hub;
use protectorate_connect_client::Client;
use protectorate_inventory::{Inventory, CARE};
use protectorate_node_monitor::Dialer;
use protectorate_wire::{
    v1::{node_agent_service, node_service},
    AuthContext, Call, Code, Result, RpcError, UnaryServer,
};
use std::{sync::Arc, time::Duration};

const CALL_TIMEOUT: Duration = Duration::from_secs(3600);

pub struct FileRelay {
    inventory: Arc<Inventory>,
    local_name: String,
    pub dialer: Dialer,
}

impl FileRelay {
    pub fn new(inventory: Arc<Inventory>, local_name: String) -> Self {
        Self {
            inventory,
            local_name,
            dialer: Dialer::default(),
        }
    }

    fn client(&self, auth: &AuthContext, name: &str) -> Result<Client> {
        self.client_with_timeout(auth, name, CALL_TIMEOUT)
    }

    pub(super) fn client_with_timeout(
        &self,
        auth: &AuthContext,
        name: &str,
        timeout: Duration,
    ) -> Result<Client> {
        self.client_for_node(Some(auth), name, timeout)
    }

    pub(super) fn internal_client_with_timeout(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<Client> {
        self.client_for_node(None, name, timeout)
    }

    fn client_for_node(
        &self,
        auth: Option<&AuthContext>,
        name: &str,
        timeout: Duration,
    ) -> Result<Client> {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(RpcError::invalid("node name is required"));
        }
        if name == self.local_name {
            return Err(super::no_local_agent());
        }
        {
            let info = self.inventory.get(&name).map_err(|error| match error {
                protectorate_inventory::Error::NotFound(_) => {
                    RpcError::new(Code::NotFound, "node not found")
                }
                other => RpcError::new(Code::Internal, other.to_string()),
            })?;
            if info.invited {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "node has not joined",
                ));
            }
            if info.tier == CARE
                && auth.is_some_and(|auth| {
                    !auth.owner
                        && !protectorate_auth::admin_on(
                            &auth.scopes.iter().map(String::as_str).collect::<Vec<_>>(),
                            &name,
                        )
                })
            {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "care node requires an owner or node admin grant",
                ));
            }
            self.dialer.client(&info, timeout)
        }
    }
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let put = Arc::clone(&hub);
    server.unary(node_service::PUT_FILE, move |call: Call<_>| {
        let hub = Arc::clone(&put);
        async move {
            let client = hub.node_files.client(&call.auth, &call.request.name)?;
            tokio::task::spawn_blocking(move || {
                client.unary(node_agent_service::PUT_FILE, &call.request)
            })
            .await
            .map_err(super::join_error)?
        }
    })?;
    server.stream(node_service::GET_FILE, move |call: Call<_>, sender| {
        let hub = Arc::clone(&hub);
        async move {
            let client = hub.node_files.client(&call.auth, &call.request.name)?;
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                client.server_stream(node_agent_service::GET_FILE, &call.request, |part| {
                    handle.block_on(sender.send(&part))
                })
            })
            .await
            .map_err(super::join_error)??;
            Ok(Default::default())
        }
    })?;
    Ok(())
}
