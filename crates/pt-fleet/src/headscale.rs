//! Headscale: its generated config, the bwrap sandbox it and its CLI run in, administration
//! through that CLI (`-o json` over the unix socket, never an API key), the control-key
//! fingerprint and database snapshots.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pt_json::{Map, Value, obj};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::files::{path_str, private_dir, write_file};

pub const POLICY: &str = include_str!("../../../go/policy.hujson");
/// The headscale user every enrolled machine is registered under before it is tagged.
pub const USER: &str = "fleet";
pub const TAG_HUB: &str = "tag:hub";
pub const TAG_NODE: &str = "tag:node";
/// Relative to the headscale dir, so the path stays under the 108-byte socket limit.
const SOCKET: &str = "headscale.sock";
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
/// Any capability version past the Noise switch makes /key return the Noise key.
const KEY_CAPVER: u32 = 130;

/// What lives in `<home>/tailnet/headscale/`.
#[derive(Clone, Debug)]
pub(crate) struct Paths {
    pub dir: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    pub fn new(cfg: &Config) -> Paths {
        let dir = cfg.dir.join("headscale");
        Paths {
            config: dir.join("headscale.yaml"),
            dir,
        }
    }
}

/// Writes headscale.yaml (JSON, which YAML accepts), the DERP map and the policy. Rewritten on
/// every start, so the config always follows `Config`.
pub(crate) fn write_config(cfg: &Config, p: &Paths) -> Result<()> {
    private_dir(&p.dir)?;
    let d = |name: &str| p.dir.join(name).to_string_lossy().into_owned();
    let derp_path = d("derp.yaml");
    let policy_path = d("policy.hujson");
    let config = obj! {
        "server_url" => cfg.server_url.as_str(),
        "listen_addr" => cfg.listen.to_string(),
        // Empty turns the debug and metrics listener off; no TLS and no
        // grpc_allow_insecure keeps the remote gRPC listener off.
        "metrics_listen_addr" => "",
        "grpc_listen_addr" => "127.0.0.1:0",
        "grpc_allow_insecure" => false,
        "trusted_proxies" => Value::Array(cfg.trusted_proxies.iter().map(|s| Value::from(s.as_str())).collect()),
        "noise" => obj! { "private_key_path" => d("noise_private.key") },
        "prefixes" => obj! { "v4" => "100.64.0.0/10", "v6" => "fd7a:115c:a1e0::/48", "allocation" => "sequential" },
        "derp" => obj! {
            "server" => obj! {
                "enabled" => true,
                "verify_clients" => true,
                // STUN is off in the map (stunport -1); headscale still binds this.
                "stun_listen_addr" => cfg.stun_listen.to_string(),
                "private_key_path" => d("derp_server_private.key"),
                "automatically_add_embedded_derp_region" => false,
            },
            "urls" => Value::Array(Vec::new()),
            "paths" => Value::Array(vec![Value::from(derp_path.as_str())]),
            "auto_update_enabled" => false,
        },
        "disable_check_updates" => true,
        "database" => obj! {
            "type" => "sqlite",
            "sqlite" => obj! { "path" => d("db.sqlite"), "write_ahead_log" => true },
        },
        "policy" => obj! { "mode" => "file", "path" => policy_path.as_str() },
        "dns" => obj! {
            "magic_dns" => false,
            "override_local_dns" => false,
            "nameservers" => obj! { "global" => Value::Array(Vec::new()) },
        },
        "unix_socket" => SOCKET,
        "unix_socket_permission" => "0700",
        "logtail" => obj! { "enabled" => false },
        "log" => obj! { "level" => "info", "format" => "text" },
    };
    let derp = &cfg.derp;
    let derp_map = format!(
        "regions:\n  900:\n    regionid: 900\n    regioncode: pt\n    regionname: Protectorate\n    nodes:\n      - name: 900a\n        regionid: 900\n        hostname: {}\n        ipv4: {}\n        derpport: {}\n        stunport: -1\n",
        yaml_str(&derp.host),
        derp.ipv4,
        derp.port
    );
    write_file(&p.config, config.to_string().as_bytes())?;
    write_file(Path::new(&derp_path), derp_map.as_bytes())?;
    write_file(Path::new(&policy_path), POLICY.as_bytes())
}

fn yaml_str(s: &str) -> String {
    Value::from(s).to_string()
}

/// headscale with `args`, inside bwrap: its own dir writable at the same path, the binary
/// read-only, fresh /dev, /proc and /tmp, and nothing else of the host or the hub's home.
pub(crate) fn command(cfg: &Config, p: &Paths, args: &[&str]) -> Result<Command> {
    let dir = path_str(&p.dir)?;
    let bin = path_str(&cfg.headscale)?;
    let config = path_str(&p.config)?;
    let mut cmd = Command::new(&cfg.bwrap);
    cmd.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--die-with-parent",
        "--new-session",
        "--as-pid-1",
        "--cap-drop",
        "ALL",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--ro-bind",
        bin,
        "/headscale",
        "--bind",
        dir,
        dir,
        "--clearenv",
        "--setenv",
        "HOME",
        dir,
        "--chdir",
        dir,
        "/headscale",
        "-c",
        config,
    ]);
    cmd.args(args);
    cmd.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    Ok(cmd)
}

/// headscale's `/health` answers 200.
pub(crate) fn healthy(cfg: &Config) -> bool {
    get(cfg, "/health").is_ok_and(|(status, _)| status == 200)
}

fn get(cfg: &Config, path: &str) -> Result<(u16, Vec<u8>)> {
    let c = pt_http::Client::tcp(cfg.listen)
        .connect_timeout(Duration::from_secs(2))
        .read_timeout(Some(Duration::from_secs(5)))
        .max_body(64 << 10);
    let r = c
        .request("GET", path, &[], &[])
        .map_err(|e| Error::new(format!("headscale GET {path}: {e}")))?;
    Ok((r.status, r.body))
}

/// `sha256:<hex>` over the 32 raw bytes of the Noise key headscale serves at `/key`, computed as
/// member computes it.
pub(crate) fn control_fingerprint(cfg: &Config) -> Result<String> {
    let path = format!("/key?v={KEY_CAPVER}");
    let (status, body) = get(cfg, &path)?;
    if status != 200 {
        return Err(Error::new(format!("headscale GET {path}: status {status}")));
    }
    let v = pt_json::parse_bytes(&body).map_err(|e| Error::new(format!("headscale /key: {e}")))?;
    let key = v
        .get("publicKey")
        .and_then(Value::as_str)
        .and_then(|k| k.strip_prefix("mkey:"))
        .ok_or_else(|| Error::new("headscale /key has no mkey: publicKey"))?;
    let raw = pt_hash::from_hex_array::<32>(key)
        .map_err(|e| Error::new(format!("headscale /key publicKey: {e:?}")))?;
    Ok(format!(
        "sha256:{}",
        pt_hash::to_hex(&pt_hash::sha256(&raw))
    ))
}

/// A node as headscale's CLI lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct HsNode {
    pub id: u64,
    /// The hostname the machine sent.
    pub hostname: String,
    /// The name the hub gave it (`nodes rename`); the tailnet DNS label.
    pub name: String,
    pub ips: Vec<String>,
    pub tags: Vec<String>,
    pub online: bool,
    pub last_seen_secs: Option<i64>,
    pub machine_key: String,
}

impl HsNode {
    fn from_json(v: &Value) -> Result<HsNode> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let strings = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(HsNode {
            id: v
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| Error::new(format!("headscale node without an id: {v}")))?,
            hostname: s("name"),
            name: s("given_name"),
            ips: strings("ip_addresses"),
            tags: strings("tags"),
            online: v.get("online").and_then(Value::as_bool).unwrap_or(false),
            last_seen_secs: v.pointer("/last_seen/seconds").and_then(Value::as_i64),
            machine_key: s("machine_key"),
        })
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// Headscale's own CLI, run in the same sandbox, against its unix socket.
#[derive(Clone)]
pub struct Admin {
    cfg: Config,
    paths: Paths,
}

impl Admin {
    pub(crate) fn new(cfg: &Config) -> Admin {
        Admin {
            cfg: cfg.clone(),
            paths: Paths::new(cfg),
        }
    }

    pub fn nodes(&self) -> Result<Vec<HsNode>> {
        let v = self.cli(&["nodes", "list"])?;
        match v {
            Value::Null => Ok(Vec::new()),
            Value::Array(a) => a.iter().map(HsNode::from_json).collect(),
            other => Err(Error::new(format!(
                "headscale nodes list: not a list: {other}"
            ))),
        }
    }

    pub fn node(&self, id: u64) -> Result<HsNode> {
        self.nodes()?
            .into_iter()
            .find(|n| n.id == id)
            .ok_or_else(|| Error::new(format!("headscale has no node {id}")))
    }

    /// Completes a pending registration under the fleet user.
    pub fn register(&self, auth_id: &str) -> Result<HsNode> {
        let v = self.cli(&["auth", "register", "--auth-id", auth_id, "--user", USER])?;
        HsNode::from_json(&v).map_err(|e| e.context(format!("registering {auth_id}")))
    }

    /// Sets the node's tags to exactly `tag`.
    pub fn tag(&self, id: u64, tag: &str) -> Result<HsNode> {
        let v = self.cli(&["nodes", "tag", "-i", &id.to_string(), "-t", tag])?;
        let n = HsNode::from_json(&v)?;
        if n.tags != [tag] {
            return Err(Error::new(format!(
                "tagging node {id} {tag}: headscale reports tags {:?}",
                n.tags
            )));
        }
        Ok(n)
    }

    pub fn rename(&self, id: u64, name: &str) -> Result<HsNode> {
        let v = self.cli(&["nodes", "rename", "-i", &id.to_string(), name])?;
        HsNode::from_json(&v)
    }

    /// Expires the node's key now: it drops off the tailnet until it registers again.
    pub fn expire(&self, id: u64) -> Result<HsNode> {
        let v = self.cli(&["nodes", "expire", "-i", &id.to_string()])?;
        HsNode::from_json(&v)
    }

    /// Removes the node from headscale. Only for an owner's explicit removal through the API.
    pub fn delete(&self, id: u64) -> Result<()> {
        self.cli(&["nodes", "delete", "-i", &id.to_string(), "--force"])
            .map(drop)
    }

    pub(crate) fn ensure_user(&self) -> Result<()> {
        let users = self.cli(&["users", "list"])?;
        let exists = users.as_array().is_some_and(|a| {
            a.iter()
                .any(|u| u.get("name").and_then(Value::as_str) == Some(USER))
        });
        if !exists {
            self.cli(&["users", "create", USER])?;
        }
        Ok(())
    }

    fn cli(&self, args: &[&str]) -> Result<Value> {
        let mut all: Vec<&str> = args.to_vec();
        all.extend(["-o", "json"]);
        let what = format!("headscale {}", args.join(" "));
        let mut cmd = command(&self.cfg, &self.paths, &all)?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::new(format!("{what}: starting bwrap: {e}")))?;
        let mut stdout = child.stdout.take().ok_or_else(|| Error::new("no stdout"))?;
        let mut stderr = child.stderr.take().ok_or_else(|| Error::new("no stderr"))?;
        let out = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stdout.read_to_end(&mut b);
            b
        });
        let err = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stderr.read_to_end(&mut b);
            b
        });
        let deadline = Instant::now() + CLI_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::new(format!(
                        "{what}: no answer within {} s",
                        CLI_TIMEOUT.as_secs()
                    )));
                }
                Err(e) => return Err(Error::new(format!("{what}: {e}"))),
            }
        };
        let stdout = out.join().unwrap_or_default();
        let stderr = err.join().unwrap_or_default();
        let v = pt_json::parse_bytes(&stdout).ok();
        if let Some(msg) = v
            .as_ref()
            .and_then(|v| v.get("error"))
            .and_then(Value::as_str)
        {
            return Err(Error::new(format!("{what}: {msg}")));
        }
        if !status.success() {
            return Err(Error::new(format!(
                "{what}: {status}: {}{}",
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        v.ok_or_else(|| {
            Error::new(format!(
                "{what}: output is not JSON: {}",
                String::from_utf8_lossy(&stdout).trim()
            ))
        })
    }
}

/// Copies headscale's state (database, keys, config) into a new directory under `snapshots/`.
/// The caller has stopped headscale, so the database is closed and its WAL folded in.
pub(crate) fn copy_state(p: &Paths, dest_root: &Path, label: &str) -> Result<PathBuf> {
    let label: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dest = dest_root.join(format!("{}-{label}", pt_api::EpochMs::now().0));
    private_dir(dest_root)?;
    fs::create_dir(&dest).map_err(|e| Error::new(format!("creating {}: {e}", dest.display())))?;
    let fill = (|| -> Result<()> {
        let mut copied = Map::new();
        for name in [
            "db.sqlite",
            "db.sqlite-wal",
            "db.sqlite-shm",
            "noise_private.key",
            "derp_server_private.key",
            "headscale.yaml",
            "derp.yaml",
            "policy.hujson",
        ] {
            let src = p.dir.join(name);
            if !src.exists() {
                continue;
            }
            let to = dest.join(name);
            let n = fs::copy(&src, &to).map_err(|e| {
                Error::new(format!(
                    "copying {} to {}: {e}",
                    src.display(),
                    to.display()
                ))
            })?;
            fs::File::open(&to)
                .and_then(|f| f.sync_all())
                .map_err(|e| Error::new(format!("syncing {}: {e}", to.display())))?;
            copied.insert(name, n);
        }
        if !copied.contains_key("db.sqlite") {
            return Err(Error::new(format!("no database in {}", p.dir.display())));
        }
        write_file(
            &dest.join("snapshot.json"),
            Value::Object(copied).to_string().as_bytes(),
        )?;
        for dir in [&dest, dest_root] {
            fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|e| Error::new(format!("syncing {}: {e}", dir.display())))?;
        }
        Ok(())
    })();
    if let Err(e) = fill {
        let _ = fs::remove_dir_all(&dest);
        return Err(e);
    }
    Ok(dest)
}
