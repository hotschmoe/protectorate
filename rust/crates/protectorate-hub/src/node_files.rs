//! Relay node file transfers without staging their contents on the hub.
use super::{input, respond, Hub};
use protectorate_connect_client::{Client, TrustRoots};
use protectorate_inventory::{Inventory, CARE};
use protectorate_node_monitor::{NodeRoute, VIRTUAL_PREFIX};
use protectorate_wire::{self as wire, Code, Result, RpcError, RpcRequest, UnaryServer};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;

const PUT: &str = "/protectorate.v1.NodeAgentService/PutFile";
const GET: &str = "/protectorate.v1.NodeAgentService/GetFile";
const CALL_TIMEOUT: Duration = Duration::from_secs(3600);

pub struct FileRelay {
    inventory: Arc<Inventory>,
    local_name: String,
    local_endpoint: Mutex<Option<String>>,
    token: Mutex<Option<Zeroizing<String>>>,
    trust: Mutex<Option<TrustRoots>>,
    route: Mutex<Option<NodeRoute>>,
}

impl FileRelay {
    pub fn new(inventory: Arc<Inventory>, local_name: String) -> Self {
        Self {
            inventory,
            local_name,
            local_endpoint: Mutex::new(None),
            token: Mutex::new(None),
            trust: Mutex::new(None),
            route: Mutex::new(None),
        }
    }

    pub fn set_token(&self, token: String) -> std::result::Result<(), &'static str> {
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err("invalid node agent token");
        }
        *self.token.lock().unwrap() = Some(Zeroizing::new(token));
        Ok(())
    }

    pub fn set_local_endpoint(&self, endpoint: String) {
        *self.local_endpoint.lock().unwrap() = Some(endpoint);
    }

    pub fn set_trust(&self, trust: TrustRoots) {
        *self.trust.lock().unwrap() = Some(trust);
    }

    pub fn set_route(&self, route: NodeRoute) {
        *self.route.lock().unwrap() = Some(route);
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
        let endpoint = if name == self.local_name {
            self.local_endpoint.lock().unwrap().clone().ok_or_else(|| {
                RpcError::new(
                    Code::Unavailable,
                    "local node agent endpoint is not configured",
                )
            })?
        } else {
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
            match self.route.lock().unwrap().clone() {
                Some(route) => route(&name, &info.address)
                    .map_err(|error| RpcError::new(Code::Unavailable, error))?,
                None => endpoint(&info.address)?,
            }
        };
        if let Some(endpoint) = endpoint.strip_prefix(VIRTUAL_PREFIX) {
            return Client::new_virtual(endpoint, timeout);
        }
        let token = self
            .token
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                RpcError::new(Code::Unavailable, "node agent token is not configured")
            })?;
        let trust = self.trust.lock().unwrap().clone();
        Client::new_with_trust(&endpoint, token.to_string(), timeout, trust.as_ref())
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
                        handle.block_on(sender.send(message))
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

fn endpoint(address: &str) -> Result<String> {
    let (scheme, authority) = if let Some(rest) = address.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = address.strip_prefix("http://") {
        ("http", rest)
    } else if address == "127.0.0.1"
        || address.starts_with("127.0.0.1:")
        || address == "[::1]"
        || address.starts_with("[::1]:")
    {
        ("http", address)
    } else {
        ("https", address)
    };
    if scheme == "http"
        && !(authority.starts_with("127.0.0.1:")
            || authority.starts_with("[::1]:")
            || authority == "127.0.0.1"
            || authority == "[::1]")
    {
        return Err(RpcError::invalid("node HTTP endpoint requires loopback"));
    }
    let has_port = if authority.starts_with('[') {
        authority.contains("]:")
    } else {
        authority.contains(':')
    };
    let authority = if has_port {
        authority.to_string()
    } else {
        format!("{authority}:7475")
    };
    Ok(format!("{scheme}://{authority}"))
}
