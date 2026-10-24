//! Relay node file transfers without staging their contents on the hub.
use super::{input, respond, Hub};
use protectorate_connect_client::Client;
use protectorate_inventory::{Inventory, CARE};
use protectorate_node_monitor::Dialer;
use protectorate_wire::{self as wire, Code, Result, RpcError, RpcRequest, UnaryServer};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const PUT: &str = "/protectorate.v1.NodeAgentService/PutFile";
const GET: &str = "/protectorate.v1.NodeAgentService/GetFile";
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

    fn client(&self, request: &RpcRequest, name: &str) -> Result<Client> {
        self.client_with_timeout(request, name, CALL_TIMEOUT)
    }

    pub(super) fn client_with_timeout(
        &self,
        request: &RpcRequest,
        name: &str,
        timeout: Duration,
    ) -> Result<Client> {
        self.client_for_node(Some(request), name, timeout)
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
        request: Option<&RpcRequest>,
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
                && request.is_some_and(|request| {
                    !request.auth.owner
                        && !protectorate_auth::admin_on(
                            &request
                                .auth
                                .scopes
                                .iter()
                                .map(String::as_str)
                                .collect::<Vec<_>>(),
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
    server.register("/protectorate.v1.NodeService/PutFile", move |request| {
        let hub = Arc::clone(&put);
        async move {
            let value = input(&request)?;
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let client = hub.node_files.client(&request, name)?;
            let reply = tokio::task::spawn_blocking(move || client.call(PUT, &value))
                .await
                .map_err(super::join_error)??;
            respond(&request, reply)
        }
    })?;
    server.register_streaming(
        "/protectorate.v1.NodeService/GetFile",
        move |request, sender| {
            let hub = Arc::clone(&hub);
            async move {
                let value = input(&request)?;
                let name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let client = hub.node_files.client(&request, name)?;
                let output = request.method.output();
                let handle = tokio::runtime::Handle::current();
                tokio::task::spawn_blocking(move || {
                    client.stream(GET, &value, |part| {
                        let bytes = serde_json::to_vec(&part)
                            .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
                        let message = wire::decode_json(output.clone(), &bytes)?;
                        handle.block_on(sender.send(&message))
                    })
                })
                .await
                .map_err(super::join_error)??;
                Ok(Default::default())
            }
        },
    )?;
    Ok(())
}
