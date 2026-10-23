//! The dispatcher against a real Rust Connect JSON server backed by the real
//! backlog, lease and inventory stores, plus fake job records. `HubService/
//! GetSleeveRuntime` is left unregistered in one server and served in the
//! other, covering hubs that do and do not expose runtime details.
use protectorate_backlog::{NewNote, Patch, Store as BacklogStore};
use protectorate_cli::{
    dispatch_endpoint, parse, run, transport_timeout, Command, NoteState, Result,
};
use protectorate_inventory::{Inventory, Node};
use protectorate_lease::{Entry, LeaseTable, Wait};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, RpcRequest, RpcResponse,
    UnaryServer,
};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

fn input(request: &RpcRequest) -> Result<Value> {
    serde_json::to_value(&request.message)
        .map_err(|error| RpcError::new(Code::Internal, format!("read request: {error}")))
}

fn respond(request: &RpcRequest, value: Value) -> Result<RpcResponse> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode response: {error}")))?;
    Ok(decode_json(request.method.output(), &bytes)?.into())
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Presence is the point: an optional proto field that was sent is Some even
/// when it is the empty string.
fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn backlog_error(error: protectorate_backlog::Error) -> RpcError {
    match error {
        protectorate_backlog::Error::Invalid(message) => {
            RpcError::new(Code::InvalidArgument, message)
        }
        protectorate_backlog::Error::NotFound(id) => {
            RpcError::new(Code::NotFound, format!("note not found: {id:?}"))
        }
        error => RpcError::new(Code::Internal, error.to_string()),
    }
}

/// Milliseconds since the epoch as the RFC3339 UTC stamps the hub sends.
fn stamp(millis: i64) -> String {
    let seconds = millis.div_euclid(1000);
    let fraction = millis.rem_euclid(1000);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{fraction:03}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_of_year = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_of_year + 2) / 5 + 1;
    let month = if month_of_year < 10 {
        month_of_year + 3
    } else {
        month_of_year - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

async fn serve(
    home: &Path,
    sleeve_runtime: bool,
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<()>,
    String,
    Arc<ChatCalls>,
    Arc<SleeveCalls>,
) {
    let backlog = Arc::new(BacklogStore::new(home));
    let leases = Arc::new(LeaseTable::memory());
    let inventory = Arc::new(Inventory::open(home.join("nodes.json"), "hubone").unwrap());
    inventory
        .add(
            Node {
                name: "compute".into(),
                address: "compute:7475".into(),
                os: "linux".into(),
                arch: "arm64".into(),
                tier: "shared".into(),
                tags: vec!["gpu".into()],
                notes: "test box".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let note = backlog
        .create(NewNote {
            title: "Fix the hub".into(),
            body: "restart the hub after the lease sweep".into(),
            pinned: true,
            project: "protectorate".into(),
            source: "owner".into(),
        })
        .unwrap();
    let note_id = note.id.clone();
    let started = Instant::now();
    let started_at = now_millis();

    let authorize: Authorizer = Arc::new(|_method, headers| {
        match headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
        {
            Some("Bearer owner-secret") => Ok(AuthContext {
                actor: "owner".into(),
                owner: true,
                ..AuthContext::default()
            }),
            _ => Err(RpcError::new(
                Code::Unauthenticated,
                "missing or invalid bearer token",
            )),
        }
    });
    let mut server = UnaryServer::new(Catalog::embedded().unwrap(), authorize);

    server
        .register("/protectorate.v1.HubService/GetStatus", move |request| {
            let uptime = 42 + started.elapsed().as_secs();
            async move {
                respond(
                    &request,
                    json!({
                        "version": "v-rust-test",
                        "hostname": "hubone",
                        "os": "linux",
                        "arch": "arm64",
                        "uptimeSeconds": uptime.to_string(),
                        "startedAt": stamp(started_at),
                    }),
                )
            }
        })
        .unwrap();
    server
        .register("/protectorate.v1.QuotaService/GetQuota", |request| async move {
            let refresh = input(&request)?["refresh"].as_bool().unwrap_or_default();
            respond(&request, json!({"quotas":[{"provider":"codex","plan":"plus","freshness":"FRESHNESS_FRESH","windows":[{"name":"5h","usedPercent":20.0}],"models":[if refresh { "gpt-6" } else { "gpt-5" }]}]}))
        })
        .unwrap();
    server
        .register("/protectorate.v1.QuotaService/Suggest", |request| async move {
            let body = input(&request)?;
            let size = string(&body, "taskSize");
            respond(&request, json!({"suggestions":[{"rank":1,"harness":"HARNESS_CODEX","provider":"codex","recommended":true,"score":42.5,"usablePercent":80.0,"expiringPercent":30.0,"reason":if size == "TASK_SIZE_LARGE" { "large task" } else { "medium task" }}]}))
        })
        .unwrap();
    if sleeve_runtime {
        server
            .register("/protectorate.v1.HubService/GetSleeveRuntime", |request| async move {
                respond(
                    &request,
                    json!({"runtime": "rust-namespaces", "helperPath": "/usr/bin/protectorate-sandbox", "overlays": true}),
                )
            })
            .unwrap();
    }

    let list_inventory = Arc::clone(&inventory);
    server
        .register("/protectorate.v1.NodeService/ListNodes", move |request| {
            let inventory = Arc::clone(&list_inventory);
            async move {
                let nodes = std::iter::once(local_node())
                    .chain(inventory.list().iter().map(node_json))
                    .map(status)
                    .collect::<Vec<_>>();
                respond(&request, json!({"nodes": nodes}))
            }
        })
        .unwrap();
    let get_inventory = Arc::clone(&inventory);
    server
        .register("/protectorate.v1.NodeService/GetNode", move |request| {
            let inventory = Arc::clone(&get_inventory);
            async move {
                let name = string(&input(&request)?, "name");
                if name == "hubone" {
                    return respond(&request, status(local_node()));
                }
                match inventory.get(&name) {
                    Ok(node) => respond(&request, status(node_json(&node))),
                    Err(error) => Err(RpcError::new(Code::NotFound, error.to_string())),
                }
            }
        })
        .unwrap();
    server
        .register(
            "/protectorate.v1.NodeService/InviteNode",
            |request| async move {
                let body = input(&request)?;
                assert_eq!(body["name"], "gpu1");
                assert_eq!(body["tier"], "shared");
                assert_eq!(body["pools"], json!(["gpu", "linux", "arm"]));
                assert_eq!(body["ttl"], "5400s");
                assert_eq!(body["installUrl"], "https://hub.example");
                respond(
                    &request,
                    json!({
                        "node": {"name":"gpu1", "tier":"shared", "invited":true},
                        "code": "secret-test-key",
                        "hubUrl": "https://hub.example",
                        "caSha256": "abc123",
                        "expiresAt": "2026-10-22T12:00:00Z",
                        "installUrl": "https://hub.example",
                    }),
                )
            },
        )
        .unwrap();

    let list_backlog = Arc::clone(&backlog);
    server
        .register(
            "/protectorate.v1.BacklogService/ListNotes",
            move |request| {
                let store = Arc::clone(&list_backlog);
                async move {
                    let body = input(&request)?;
                    let notes = store
                        .list(protectorate_backlog::Filter {
                            project: string(&body, "project"),
                            query: string(&body, "query"),
                            state: string(&body, "state"),
                        })
                        .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
                    respond(
                        &request,
                        json!({"notes": notes.iter().map(note_json).collect::<Vec<_>>() }),
                    )
                }
            },
        )
        .unwrap();
    let get_backlog = Arc::clone(&backlog);
    server
        .register("/protectorate.v1.BacklogService/GetNote", move |request| {
            let store = Arc::clone(&get_backlog);
            async move {
                match store.get(&string(&input(&request)?, "id")) {
                    Ok(note) => respond(&request, json!({"note": note_json(&note)})),
                    Err(error) => Err(RpcError::new(Code::NotFound, error.to_string())),
                }
            }
        })
        .unwrap();
    let create_backlog = Arc::clone(&backlog);
    server
        .register(
            "/protectorate.v1.BacklogService/CreateNote",
            move |request| {
                let store = Arc::clone(&create_backlog);
                async move {
                    let body = input(&request)?;
                    let note = store
                        .create(NewNote {
                            title: string(&body, "title"),
                            body: string(&body, "body"),
                            pinned: body.get("pinned").and_then(Value::as_bool).unwrap_or(false),
                            project: string(&body, "project"),
                            source: request.auth.actor.clone(),
                        })
                        .map_err(backlog_error)?;
                    respond(&request, json!({"note": note_json(&note)}))
                }
            },
        )
        .unwrap();
    let update_backlog = Arc::clone(&backlog);
    server
        .register(
            "/protectorate.v1.BacklogService/UpdateNote",
            move |request| {
                let store = Arc::clone(&update_backlog);
                async move {
                    let body = input(&request)?;
                    let (note, _) = store
                        .update(
                            &string(&body, "id"),
                            Patch {
                                title: optional_string(&body, "title"),
                                body: optional_string(&body, "body"),
                                pinned: body.get("pinned").and_then(Value::as_bool),
                                project: optional_string(&body, "project"),
                                state: optional_string(&body, "state"),
                                resolution: optional_string(&body, "resolution"),
                                done_by: request.auth.actor.clone(),
                            },
                        )
                        .map_err(backlog_error)?;
                    respond(&request, json!({"note": note_json(&note)}))
                }
            },
        )
        .unwrap();

    let now = now_millis();
    let jobs = Arc::new(job_records(now));
    let list_jobs = Arc::clone(&jobs);
    server
        .register("/protectorate.v1.JobService/ListJobs", move |request| {
            let jobs = Arc::clone(&list_jobs);
            async move {
                let body = input(&request)?;
                let state = string(&body, "state");
                let limit = body.get("limit").and_then(Value::as_i64).unwrap_or(50);
                if !(0..=1000).contains(&limit) {
                    return Err(RpcError::invalid("invalid limit"));
                }
                let limit = if limit == 0 { 50 } else { limit as usize };
                let jobs = jobs
                    .iter()
                    .filter(|job| state.is_empty() || job["state"] == state)
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                respond(&request, json!({"jobs": jobs}))
            }
        })
        .unwrap();
    let get_jobs = Arc::clone(&jobs);
    server
        .register("/protectorate.v1.JobService/GetJob", move |request| {
            let jobs = Arc::clone(&get_jobs);
            async move {
                let id = string(&input(&request)?, "id");
                match jobs.iter().find(|job| job["id"] == id) {
                    Some(job) => respond(&request, job.clone()),
                    None => Err(RpcError::new(Code::NotFound, format!("no job {id}"))),
                }
            }
        })
        .unwrap();
    let cancel_jobs = Arc::clone(&jobs);
    server
        .register("/protectorate.v1.JobService/CancelJob", move |request| {
            let jobs = Arc::clone(&cancel_jobs);
            async move {
                let id = string(&input(&request)?, "id");
                let mut job = jobs
                    .iter()
                    .find(|job| job["id"] == id)
                    .cloned()
                    .ok_or_else(|| RpcError::new(Code::NotFound, format!("no job {id}")))?;
                job["state"] = json!("JOB_STATE_CANCELED");
                job["error"] = json!("canceled by the owner");
                job["finishedAt"] = json!(stamp(now_millis()));
                respond(&request, job)
            }
        })
        .unwrap();

    leases
        .acquire("sleeve-x", Duration::from_secs(3600), "compute:gpu1", now)
        .unwrap();
    leases
        .acquire_wait(
            Wait {
                holder: "waiter".into(),
                notify: String::new(),
                resource: "compute".into(),
                request: "compute".into(),
                ttl: Duration::from_secs(600),
                priority: 0,
            },
            now,
        )
        .unwrap();
    server
        .register("/protectorate.v1.LeaseService/ListLeases", move |request| {
            let table = Arc::clone(&leases);
            async move {
                let entries = table
                    .list(now_millis())
                    .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
                respond(
                    &request,
                    json!({"leases": entries.iter().map(lease_json).collect::<Vec<_>>() }),
                )
            }
        })
        .unwrap();

    let chat_calls = Arc::new(ChatCalls::default());
    register_chat(&mut server, Arc::clone(&chat_calls));
    server.register("/protectorate.v1.DirectoryService/ListProjects", move |request| async move {
        let sleeve = string(&input(&request)?, "sleeve");
        let projects = if sleeve.is_empty() || sleeve == "sleeve-x" {
            vec![json!({"id":"example.com/acme/app","display":"acme/app","owners":[{"sleeve":"sleeve-x","branch":"main","head":"123456789abcdef","workspace":"/work/app"}],"provides":["cargo:app"],"dependencyCount":2,"dependsOn":[{"display":"acme/lib","via":["cargo:lib"]}]})]
        } else { Vec::new() };
        respond(&request, json!({"projects":projects}))
    }).unwrap();
    server.register("/protectorate.v1.DirectoryService/GetProject", move |request| async move {
        let project = string(&input(&request)?, "project");
        if project != "app" { return Err(RpcError::new(Code::NotFound, "project not found")); }
        respond(&request, json!({"project":{"id":"example.com/acme/app","display":"acme/app","owners":[{"sleeve":"sleeve-x","branch":"main","head":"123456789abcdef","workspace":"/work/app"}],"provides":["cargo:app"],"dependencyCount":2,"dependsOn":[{"display":"acme/lib","via":["cargo:lib"]}]}}))
    }).unwrap();
    let sleeve_calls = Arc::new(SleeveCalls::default());
    register_sleeves(&mut server, Arc::clone(&sleeve_calls));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    (addr, task, note_id, chat_calls, sleeve_calls)
}

/// What the fake ChatService saw, so a test can check the request the CLI
/// actually put on the wire.
#[derive(Default)]
struct ChatCalls {
    send: std::sync::Mutex<Vec<(String, String, String)>>,
    wait: std::sync::Mutex<Vec<(String, String, String)>>,
    report: std::sync::Mutex<Vec<(String, String)>>,
}

/// ChatService/Send and ChatService/Wait with the Rust hub's own answers:
/// sleeve-x is an idle chat sleeve, busy-x is mid-turn, reported-x has a
/// result, exited-x is gone and ghost never existed.
fn register_chat(server: &mut UnaryServer, chat: Arc<ChatCalls>) {
    let report_chat = Arc::clone(&chat);
    server
        .register("/protectorate.v1.ChatService/Report", move |request| {
            let chat = Arc::clone(&report_chat);
            async move {
                let body = input(&request)?;
                chat.report.lock().unwrap().push((string(&body, "status"), string(&body, "summary")));
                respond(&request, json!({"result":{"status":"REPORT_STATUS_COMPLETE","summary":string(&body,"summary"),"chatSeq":"13"}}))
            }
        })
        .unwrap();
    let send_chat = Arc::clone(&chat);
    server
        .register("/protectorate.v1.ChatService/Send", move |request| {
            let chat = Arc::clone(&send_chat);
            async move {
                let body = input(&request)?;
                let name = string(&body, "sleeve");
                let text = string(&body, "text");
                let request_id = string(&body, "requestId");
                if name != "sleeve-x" && name != "busy-x" && name != "exited-x" {
                    return Err(RpcError::new(Code::NotFound, format!("no sleeve {name}")));
                }
                if name == "exited-x" {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "chat sleeve is not running",
                    ));
                }
                if text.is_empty() {
                    return Err(RpcError::invalid("text is required"));
                }
                chat.send.lock().unwrap().push((name, text, request_id));
                respond(&request, json!({"seq": "7"}))
            }
        })
        .unwrap();
    server
        .register("/protectorate.v1.ChatService/Wait", move |request| {
            let chat = Arc::clone(&chat);
            async move {
                let body = input(&request)?;
                let name = string(&body, "sleeve");
                let until = string(&body, "until");
                let timeout = string(&body, "timeout");
                chat.wait
                    .lock()
                    .unwrap()
                    .push((name.clone(), until.clone(), timeout.clone()));
                let seconds: f64 = timeout
                    .trim_end_matches('s')
                    .parse()
                    .map_err(|_| RpcError::invalid("timeout must be a duration"))?;
                match name.as_str() {
                    "exited-x" => Err(RpcError::new(
                        Code::FailedPrecondition,
                        "sleeve exited before wait condition",
                    )),
                    "ghost" => Err(RpcError::new(Code::NotFound, format!("no sleeve {name}"))),
                    "busy-x" => {
                        tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
                        Err(RpcError::new(
                            Code::DeadlineExceeded,
                            format!("wait timed out after {seconds}s"),
                        ))
                    }
                    "reported-x" if until == "WAIT_UNTIL_REPORT" => respond(
                        &request,
                        json!({
                            "sleeve": name,
                            "result": {
                                "status": "REPORT_STATUS_COMPLETE",
                                "summary": "tests pass", "time": stamp(now_millis()),
                                "chatSeq": "9", "briefSeq": "4",
                            },
                        }),
                    ),
                    _ if until == "WAIT_UNTIL_REPORT" => {
                        tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
                        Err(RpcError::new(
                            Code::DeadlineExceeded,
                            format!("wait timed out after {seconds}s"),
                        ))
                    }
                    _ => respond(&request, json!({"sleeve": name})),
                }
            }
        })
        .unwrap();
}

/// What the fake SleeveService and SessionService saw, so a test can check
/// the requests the CLI actually put on the wire.
#[derive(Default)]
struct SleeveCalls {
    kills: std::sync::Mutex<Vec<(String, bool)>>,
    panes: std::sync::Mutex<Vec<(String, i64)>>,
    lifecycle: std::sync::Mutex<Vec<(String, Value)>>,
}

/// SleeveService and SessionService/CapturePane with the Rust hub's own
/// answers: running-x is a live chat sleeve, exited-x ended with exit 3 and
/// keeps a capturable pane, killed-x was killed and oom-x died of OOM with no
/// session left to read.
fn register_sleeves(server: &mut UnaryServer, calls: Arc<SleeveCalls>) {
    for (method, response) in [
        (
            "SpawnSleeve",
            json!({"sleeve":{"name":"worker","harness":"HARNESS_CODEX","mode":"SLEEVE_MODE_CHAT"}}),
        ),
        (
            "SpawnEnvoy",
            json!({"sleeve":{"name":"envoy","harness":"HARNESS_CODEX","mode":"SLEEVE_MODE_CHAT"},"token":{"id":"tok-1","scopes":["message_*"]},"workspace":"/hub/envoy","reason":"chosen explicitly"}),
        ),
        (
            "ResleeveSleeve",
            json!({"sleeve":{"name":"worker","harness":"HARNESS_PI","mode":"SLEEVE_MODE_CHAT","handoff":{"fromHarness":"codex","toHarness":"pi","fromSessionId":"old","path":"/workspace/.protectorate/HANDOFF.md"}}}),
        ),
        (
            "PruneSleeves",
            json!({"entries":[{"sleeve":"worker","kind":"cache","path":"/hub/cache","bytes":"1024","eligible":true}]}),
        ),
    ] {
        let calls = Arc::clone(&calls);
        server
            .register(
                &format!("/protectorate.v1.SleeveService/{method}"),
                move |request| {
                    let calls = Arc::clone(&calls);
                    let response = response.clone();
                    async move {
                        calls
                            .lifecycle
                            .lock()
                            .unwrap()
                            .push((method.into(), input(&request)?));
                        respond(&request, response)
                    }
                },
            )
            .unwrap();
    }
    let sleeves = Arc::new(sleeve_records());
    let list_sleeves = Arc::clone(&sleeves);
    server
        .register(
            "/protectorate.v1.SleeveService/ListSleeves",
            move |request| {
                let sleeves = Arc::clone(&list_sleeves);
                async move {
                    respond(
                        &request,
                        json!({"sleeves": sleeves.iter().cloned().collect::<Vec<_>>()}),
                    )
                }
            },
        )
        .unwrap();
    let get_sleeves = Arc::clone(&sleeves);
    server
        .register("/protectorate.v1.SleeveService/GetSleeve", move |request| {
            let sleeves = Arc::clone(&get_sleeves);
            async move {
                let name = string(&input(&request)?, "name");
                match sleeves.iter().find(|sleeve| sleeve["name"] == name) {
                    Some(sleeve) => respond(&request, json!({"sleeve": sleeve.clone()})),
                    None => Err(RpcError::new(Code::NotFound, format!("no sleeve {name}"))),
                }
            }
        })
        .unwrap();
    let kill_sleeves = Arc::clone(&sleeves);
    let kill_calls = Arc::clone(&calls);
    server
        .register(
            "/protectorate.v1.SleeveService/KillSleeve",
            move |request| {
                let sleeves = Arc::clone(&kill_sleeves);
                let calls = Arc::clone(&kill_calls);
                async move {
                    let body = input(&request)?;
                    let name = string(&body, "name");
                    let drop_caches = body
                        .get("dropCaches")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if !sleeves.iter().any(|sleeve| sleeve["name"] == name) {
                        return Err(RpcError::new(Code::NotFound, format!("no sleeve {name}")));
                    }
                    calls.kills.lock().unwrap().push((name, drop_caches));
                    respond(&request, json!({}))
                }
            },
        )
        .unwrap();
    let pane_calls = Arc::clone(&calls);
    server
        .register(
            "/protectorate.v1.SessionService/CapturePane",
            move |request| {
                let calls = Arc::clone(&pane_calls);
                async move {
                    let body = input(&request)?;
                    let name = string(&body, "name");
                    let lines = body
                        .get("lines")
                        .and_then(Value::as_i64)
                        .unwrap_or_default();
                    calls.panes.lock().unwrap().push((name.clone(), lines));
                    match name.as_str() {
                        "sleeve-exited-x" => respond(&request, json!({"text": pane_text()})),
                        _ => Err(RpcError::new(Code::NotFound, format!("no session {name}"))),
                    }
                }
            },
        )
        .unwrap();
}

fn sleeve_records() -> Vec<Value> {
    vec![
        json!({
            "name": "running-x", "harness": "HARNESS_PI", "mode": "SLEEVE_MODE_CHAT",
            "state": "SLEEVE_STATE_RUNNING", "workspace": "/home/u/github/app",
            "session": "sleeve-running-x", "createdAt": "2026-01-02T15:04:05Z",
            "parent": "owner", "gitBranch": "main", "supervised": true,
            "limits": {"cpuPercent": 200, "memoryBytes": "8589934592"},
            "usage": {"cpuPercent": 12.4, "cpuSeconds": 30.4, "memoryCurrentBytes": "1048576",
                "memoryPeakBytes": "2097152", "pids": 7, "oomKills": "0",
                "workspaceDiskBytes": "4096", "homeDiskBytes": "3221225472",
                "cacheDiskBytes": "1073741824", "goBuildCacheDiskBytes": "0",
                "sampledAt": "2026-01-02T15:05:00Z"},
            "readOnly": [{"source": "/etc/ssl", "target": "/etc/ssl"}],
            "chat": {"state": "CHAT_STATE_IDLE", "model": "gpt-5.2-codex",
                "reasoningEffort": "high", "harnessSessionId": "ses_1",
                "provider": "openai", "localModel": false, "contextTokens": "12000",
                "contextWindow": "272000", "lastSeq": "4",
                "lastActivityAt": "2026-01-02T15:05:00Z",
                "settings": {"requested": {"model": "gpt-5.2-codex", "effort": "high"},
                    "nativeDefaults": {"model": "gpt-5.1-codex-mini", "effort": "medium"}}},
            "result": {"status": "REPORT_STATUS_COMPLETE", "summary": "tests pass",
                "time": "2026-01-02T15:04:40Z", "chatSeq": "9", "briefSeq": "4"},
            "handoff": {"fromHarness": "codex", "fromSessionId": "ses_0",
                "toHarness": "pi", "summarizerHarness": "claude",
                "summarizerProvider": "anthropic", "summarizerModel": "claude-opus",
                "path": ".protectorate/HANDOFF.md", "at": "2026-01-02T14:00:00Z"},
        }),
        json!({
            "name": "exited-x", "harness": "HARNESS_SHELL", "mode": "SLEEVE_MODE_TERMINAL",
            "state": "SLEEVE_STATE_EXITED", "exitCode": 3, "exitSignal": 0,
            "workspace": "/home/u/github/tool", "session": "sleeve-exited-x",
            "createdAt": "2026-01-02T14:00:00Z", "exitedAt": "2026-01-02T14:10:00Z",
            "parent": "owner", "args": ["sh", "-c", "exit 3"], "limits": {},
            "exitAfterIdle": "0s",
            "usage": {"cpuPercent": 0.0, "cpuSeconds": 1.5, "memoryCurrentBytes": "0",
                "memoryPeakBytes": "524288", "pids": 0, "oomKills": "0",
                "workspaceDiskBytes": "2048", "homeDiskBytes": "4096",
                "cacheDiskBytes": "0", "goBuildCacheDiskBytes": "0",
                "sampledAt": "2026-01-02T14:10:00Z", "diskSampledAt": "2026-01-02T14:10:00Z"},
        }),
        json!({
            "name": "killed-x", "harness": "HARNESS_CLAUDE", "mode": "SLEEVE_MODE_CHAT",
            "state": "SLEEVE_STATE_EXITED", "exitCode": 137, "exitSignal": 9,
            "killed": true, "workspace": "/home/u/github/killed",
            "session": "sleeve-killed-x", "createdAt": "2026-01-02T13:00:00Z",
            "exitedAt": "2026-01-02T13:30:00Z", "parent": "sleeve:envoy",
            "limits": {"cpuPercent": 100}, "usage": {},
            "chat": {"state": "CHAT_STATE_EXITED"},
        }),
        json!({
            "name": "oom-x", "harness": "HARNESS_CODEX", "mode": "SLEEVE_MODE_CHAT",
            "state": "SLEEVE_STATE_EXITED", "exitCode": 137, "oomKilled": true,
            "workspace": "/home/u/github/oom", "session": "sleeve-oom-x",
            "createdAt": "2026-01-02T12:00:00Z", "exitedAt": "2026-01-02T12:30:00Z",
            "parent": "owner", "limits": {"memoryBytes": "536870912"}, "usage": {},
            "chat": {"state": "CHAT_STATE_EXITED", "model": "", "reasoningEffort": ""},
        }),
    ]
}

/// A kept pane: 25 colored, tab-separated lines, so a tail test can see the
/// SGR stripping, the tab expansion and the 20-line cut.
fn pane_text() -> String {
    let mut text = String::new();
    for line in 1..=25 {
        text.push_str(&format!(
            "\u{1b}[31moutput line {line}\ttrailing\u{1b}[0m\n"
        ));
    }
    text
}

/// The hub's own shape for the jobs this fake hub has already run.
fn job_records(now: i64) -> Vec<Value> {
    vec![
        json!({
            "id": "job-2", "state": "JOB_STATE_RUNNING",
            "target": {"node": "compute"}, "node": "compute",
            "source": {"git": {"repo": "/srv/git/protectorate", "commit": "abc123"}},
            "command": ["cargo", "test", "--all-features", "--workspace", "--",
                        "--exact", "hub::jobs::test"],
            "timeout": "3600s", "submittedAt": stamp(now - 1_800_000),
            "startedAt": stamp(now - 1_770_000), "sourceBytes": "4096",
            "transferTime": "1.5s", "runTime": "12.4s",
            "mode": "JOB_MODE_SANDBOXED", "isolation": "bwrap",
        }),
        json!({
            "id": "job-1", "state": "JOB_STATE_FAILED", "target": {"node": "win1"},
            "node": "win1", "command": ["protectorate", "node", "ls"], "timeout": "3600s",
            "submittedAt": stamp(now - 86_400_000), "startedAt": stamp(now - 86_300_000),
            "finishedAt": stamp(now - 86_000_000), "exitCode": 101, "error": "exit 101",
            "runTime": "300s", "logBytes": "2048", "mode": "JOB_MODE_PLAIN",
            "isolation": "none", "workspaceReused": true,
            "artifacts": [{"path": "report.txt", "sizeBytes": "1024"}],
        }),
    ]
}

fn local_node() -> Value {
    json!({
        "name": "hubone", "address": "local", "os": "linux", "arch": "arm64",
        "tier": "shared", "local": true,
    })
}

fn node_json(node: &Node) -> Value {
    let mut out = serde_json::to_value(node).unwrap();
    for private in [
        "codeSha256",
        "codeExpires",
        "certId",
        "certExpires",
        "publicKey",
    ] {
        out.as_object_mut().unwrap().remove(private);
    }
    out
}

/// The hub's own shape for a node it has never polled.
fn status(node: Value) -> Value {
    json!({"node": node, "error": "awaiting first poll"})
}

fn note_json(note: &protectorate_backlog::Note) -> Value {
    let mut out = json!({
        "id": note.id, "title": note.title, "body": note.body,
        "createdAt": stamp(note.created_at.timestamp_millis()),
        "updatedAt": stamp(note.updated_at.timestamp_millis()),
        "pinned": note.pinned, "project": note.project, "source": note.source,
        "state": note.state,
    });
    if let Some(done_at) = note.done_at {
        out["completion"] = json!({
            "doneAt": stamp(done_at.timestamp_millis()),
            "doneBy": note.done_by, "resolution": note.resolution,
        });
    }
    out
}

fn lease_json(entry: &Entry) -> Value {
    match entry {
        Entry::Active(lease) => json!({
            "id": lease.id, "resource": lease.resource.to_string(), "node": lease.resource.node,
            "holder": lease.holder, "ttl": format!("{}s", lease.ttl.as_secs()),
            "acquiredAt": stamp(lease.acquired_at), "expiresAt": stamp(lease.expires_at),
        }),
        Entry::Queued { ticket, position } => json!({
            "id": ticket.id, "resource": ticket.resource, "holder": ticket.holder,
            "state": "LEASE_STATE_QUEUED", "queuePosition": position,
            "expiresAt": stamp(ticket.enqueued_at + 3_600_000),
        }),
    }
}

fn code(result: Result<String>) -> std::result::Result<String, Code> {
    result.map_err(|error| error.code)
}

type Calls = std::vec::IntoIter<std::result::Result<String, Code>>;

fn ok(calls: &mut Calls, label: &str) -> String {
    match calls.next().unwrap() {
        Ok(text) => text,
        Err(code) => panic!("expected {label} to succeed, got {code:?}"),
    }
}

fn fail(calls: &mut Calls, label: &str) -> Code {
    match calls.next().unwrap() {
        Ok(text) => panic!("expected {label} to fail, got {text}"),
        Err(code) => code,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn quota_and_suggest_use_real_connect_routes() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, _, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let output = tokio::task::spawn_blocking(move || {
        let quota = dispatch_endpoint(&base, OWNER, ["quota", "--refresh"])?;
        let json_quota = dispatch_endpoint(&base, OWNER, ["quota", "--refresh", "--json"])?;
        let suggest = dispatch_endpoint(&base, OWNER, ["quota", "suggest", "--for", "large"])?;
        let default_suggest = dispatch_endpoint(&base, OWNER, ["quota", "suggest"])?;
        Ok::<_, RpcError>((quota, json_quota, suggest, default_suggest))
    })
    .await
    .unwrap()
    .unwrap();
    assert!(output.0.contains("codex"));
    assert!(output.0.contains("20.0%"));
    assert!(output.1.contains("gpt-6"));
    assert!(output.2.contains("large task"));
    assert!(output.2.contains("80%"));
    assert!(output.3.contains("medium task"));
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn node_invite_sends_options_and_reports_registration_limit() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, _, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let result = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &base,
            OWNER,
            [
                "node",
                "invite",
                "gpu1",
                "--pool",
                "gpu,linux",
                "--pool=arm",
                "--expires",
                "90m",
                "--url",
                "https://hub.example",
            ],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(result.contains("secret-test-key"), "{result}");
    assert!(result.contains("protectorate-rs node install"), "{result}");
    assert!(
        result.contains("--ca-cert ca.pem") && result.contains("abc123"),
        "{result}"
    );
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_commands_read_a_real_connect_server() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, note_id, _, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let calls = tokio::task::spawn_blocking(move || {
        let commands: Vec<Vec<&str>> = vec![
            vec!["--json", "status"],
            vec!["status"],
            vec!["node", "ls"],
            vec!["--json", "node", "show", "compute"],
            vec!["node", "show", "stranger"],
            vec!["backlog", "ls"],
            vec!["--json", "backlog", "list", "--project", "protectorate"],
            vec!["backlog", "ls", "--state", "done"],
            vec!["backlog", "show", note_id.as_str()],
            vec!["lease", "ls"],
            vec!["--json", "lease", "list"],
            vec!["job", "ls"],
            vec!["--json", "job", "ls", "--state", "running", "--limit", "1"],
            vec!["job", "ls", "--state", "succeeded"],
            vec!["job", "show", "job-1"],
            vec!["--json", "job", "show", "job-2"],
            vec!["job", "show", "no-such-job"],
            vec!["job", "cancel", "job-2"],
            vec!["--json", "job", "cancel", "job-1"],
            vec!["job", "cancel", "no-such-job"],
        ];
        let mut out: Vec<std::result::Result<String, Code>> = commands
            .iter()
            .map(|args| code(dispatch_endpoint(&base, OWNER, args.iter())))
            .collect();
        out.push(code(dispatch_endpoint(&base, "wrong-token", ["status"])));
        out
    })
    .await
    .unwrap();
    server.abort();

    let mut calls = calls.into_iter();

    let status: Value = serde_json::from_str(&ok(&mut calls, "json status")).unwrap();
    assert_eq!(status["version"], "v-rust-test");
    assert_eq!(status["hostname"], "hubone");
    assert!(status["uptimeSeconds"].is_string(), "{status}");

    let status = ok(&mut calls, "status");
    assert!(status.contains("v-rust-test"), "{status}");
    assert!(status.contains("platform"), "{status}");
    assert!(status.contains("uptime"), "{status}");
    assert!(status.contains("started"), "{status}");
    assert!(
        !status.contains("bwrap"),
        "an absent runtime is skipped: {status}"
    );

    let nodes = ok(&mut calls, "node ls");
    assert!(nodes.contains("NAME"), "{nodes}");
    assert!(nodes.contains("hubone*"), "{nodes}");
    assert!(nodes.contains("compute"), "{nodes}");
    assert!(nodes.contains("linux/arm64"), "{nodes}");
    assert!(nodes.contains("pending"), "{nodes}");
    assert!(nodes.contains("* the hub's own node"), "{nodes}");

    let node: Value = serde_json::from_str(&ok(&mut calls, "json node show")).unwrap();
    assert_eq!(node["node"]["name"], "compute");
    assert_eq!(node["node"]["tier"], "shared");
    assert_eq!(node["error"], "awaiting first poll");

    assert_eq!(fail(&mut calls, "node show stranger"), Code::NotFound);

    let notes = ok(&mut calls, "backlog ls");
    assert!(notes.contains("Fix the hub"), "{notes}");
    assert!(notes.contains("pin"), "{notes}");
    assert!(notes.contains("protectorate"), "{notes}");

    let notes = ok(&mut calls, "json backlog list");
    let lines: Vec<Value> = notes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["title"], "Fix the hub");
    assert_eq!(lines[0]["state"], "open");

    assert_eq!(
        ok(&mut calls, "backlog ls --state done"),
        "",
        "no done notes"
    );

    let shown = ok(&mut calls, "backlog show");
    assert!(
        shown.contains("restart the hub after the lease sweep"),
        "{shown}"
    );
    assert!(shown.contains("open"), "{shown}");

    let leases = ok(&mut calls, "lease ls");
    assert!(leases.contains("ID"), "{leases}");
    assert!(leases.contains("compute:gpu1"), "{leases}");
    assert!(leases.contains("sleeve-x"), "{leases}");
    assert!(leases.contains("waiter"), "{leases}");
    assert!(leases.contains("queued #1"), "{leases}");
    assert!(
        leases.contains("1h0m0s") || leases.contains("59m59s"),
        "{leases}"
    );

    let leases: Value = serde_json::from_str(&ok(&mut calls, "json lease list")).unwrap();
    let entries = leases["leases"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].get("queuePosition").is_none(), "{entries:?}");
    assert_eq!(entries[1]["state"], "LEASE_STATE_QUEUED");
    assert_eq!(entries[1]["queuePosition"], 1);

    let jobs = ok(&mut calls, "job ls");
    let rows: Vec<&str> = jobs.lines().collect();
    assert_eq!(rows.len(), 3, "{jobs}");
    assert!(rows[0].starts_with("ID"), "{jobs}");
    assert!(rows[1].starts_with("job-2"), "{}", rows[1]);
    assert!(rows[1].contains("running"), "{}", rows[1]);
    assert!(rows[1].contains("compute"), "{}", rows[1]);
    assert!(rows[1].contains("30m"), "{}", rows[1]);
    assert!(rows[1].contains("12s"), "{}", rows[1]);
    assert!(rows[1].ends_with("--exac~"), "{}", rows[1]);
    assert!(rows[2].starts_with("job-1"), "{}", rows[2]);
    assert!(rows[2].contains("failed"), "{}", rows[2]);
    assert!(rows[2].contains("protectorate node ls"), "{}", rows[2]);

    let jobs: Vec<Value> = ok(&mut calls, "json job ls --state running")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(jobs.len(), 1, "--limit 1: {jobs:?}");
    assert_eq!(jobs[0]["id"], "job-2", "{jobs:?}");
    assert_eq!(jobs[0]["state"], "JOB_STATE_RUNNING", "{jobs:?}");

    let empty = ok(&mut calls, "job ls --state succeeded");
    assert_eq!(empty.lines().count(), 1, "{empty}");
    assert!(empty.starts_with("ID"), "{empty}");

    let job = ok(&mut calls, "job show");
    assert!(job.contains("id"), "{job}");
    assert!(job.contains("failed"), "{job}");
    assert!(job.contains("plain"), "{job}");
    assert!(job.contains("exit code"), "{job}");
    assert!(job.contains("101"), "{job}");
    assert!(job.contains("exit 101"), "{job}");
    assert!(job.contains("5m0s"), "{job}");
    assert!(job.contains("2.0KiB"), "{job}");
    assert!(job.contains("report.txt"), "{job}");
    assert_eq!(
        job.lines()
            .filter(|line| line.starts_with("source"))
            .count(),
        1,
        "a job with no source has no source row: {job}"
    );

    let job: Value = serde_json::from_str(&ok(&mut calls, "json job show")).unwrap();
    assert_eq!(job["id"], "job-2");
    assert_eq!(job["node"], "compute");
    assert_eq!(job["source"]["git"]["commit"], "abc123");

    assert_eq!(fail(&mut calls, "job show no-such-job"), Code::NotFound);

    assert_eq!(
        ok(&mut calls, "job cancel"),
        "job job-2 canceled\n",
        "the hub's record after cancel"
    );

    let job: Value = serde_json::from_str(&ok(&mut calls, "json job cancel")).unwrap();
    assert_eq!(job["id"], "job-1");
    assert_eq!(job["state"], "JOB_STATE_CANCELED");
    assert_eq!(job["error"], "canceled by the owner");

    assert_eq!(fail(&mut calls, "job cancel no-such-job"), Code::NotFound);

    assert_eq!(
        fail(&mut calls, "status with a bad token"),
        Code::Unauthenticated
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn status_adds_the_sleeve_runtime_when_the_hub_reports_it() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, _, _) = serve(home.path(), true).await;
    let base = format!("http://{addr}");
    let text =
        tokio::task::spawn_blocking(move || dispatch_endpoint(&base, OWNER, ["status"]).unwrap())
            .await
            .unwrap();
    server.abort();
    assert!(text.contains("rust-namespaces"), "{text}");
    assert!(text.contains("/usr/bin/protectorate-sandbox"), "{text}");
    assert!(!text.contains("bwrap"), "{text}");
    assert!(text.contains("overlays"), "{text}");
}

#[test]
fn run_resolves_the_token_before_any_connection() {
    let missing = "/definitely/missing/protectorate-token";
    let error = run(["--token-file", missing, "status"]).unwrap_err();
    assert_eq!(error.code, Code::Internal);
    assert!(error.message.contains("read token file"), "{error}");

    let error = run(["status"]).unwrap_err();
    assert!(
        error.code == Code::Unauthenticated || error.code == Code::Internal,
        "{error}"
    );
}

#[test]
fn stamps_render_the_day_the_hub_meant() {
    assert_eq!(stamp(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(stamp(951_868_799_500), "2000-02-29T23:59:59.500Z");
    assert_eq!(stamp(-1), "1969-12-31T23:59:59.999Z");
}

#[test]
fn help_and_filters_parse_without_touching_the_network() {
    assert_eq!(parse(["--help"]).unwrap().command, Command::Help);
    let help = run(["--help"]).unwrap();
    assert!(help.starts_with("Usage:"), "{help}");
    assert!(help.contains("backlog show <id>"), "{help}");
    assert!(help.contains("backlog add [title]"), "{help}");
    assert!(help.contains("backlog done <id>"), "{help}");
    assert!(help.contains("backlog reopen <id>"), "{help}");
    assert!(help.contains("job cancel <id>"), "{help}");
    assert_eq!(
        parse(["backlog", "ls", "--all"]).unwrap().command,
        Command::BacklogList(protectorate_cli::BacklogFilter {
            state: NoteState::All,
            ..Default::default()
        })
    );
}

#[test]
fn command_help_works_without_hub_credentials() {
    for args in [
        vec!["help", "node", "update"],
        vec!["node", "update", "--help"],
        vec!["--hub", "http://invalid", "node", "update", "-h"],
    ] {
        let help = run(args).unwrap();
        assert!(help.starts_with("Usage:\n"), "{help}");
        assert!(help.contains("node update <name> | --all"), "{help}");
        assert!(!help.contains("backlog show"), "{help}");
    }
    let help = run(["help", "node"]).unwrap();
    assert!(help.contains("node update"), "{help}");
    assert!(help.contains("node invite"), "{help}");
    assert!(!help.contains("sleeve spawn"), "{help}");
    let error = run(["help", "imaginary"]).unwrap_err();
    assert!(error.message.contains("unknown command"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn who_reads_directory_over_http() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, _, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let results = tokio::task::spawn_blocking(move || {
        [
            dispatch_endpoint(&base, OWNER, ["who"]),
            dispatch_endpoint(&base, OWNER, ["who", "sleeve-x"]),
            dispatch_endpoint(&base, OWNER, ["who", "app"]),
            dispatch_endpoint(&base, OWNER, ["--json", "who", "app"]),
            dispatch_endpoint(&base, OWNER, ["who", "missing"]),
        ]
    })
    .await
    .unwrap();
    server.abort();
    assert!(results[0]
        .as_ref()
        .unwrap()
        .contains("acme/app\tsleeve-x\tacme/lib"));
    assert!(results[1]
        .as_ref()
        .unwrap()
        .contains("sleeve\tsleeve-x\tmain 12345678"));
    assert!(results[2]
        .as_ref()
        .unwrap()
        .contains("depends on\tacme/lib\tvia cargo:lib"));
    let json: Value = serde_json::from_str(results[3].as_ref().unwrap()).unwrap();
    assert_eq!(json["project"]["id"], "example.com/acme/app");
    assert_eq!(results[4].as_ref().unwrap_err().code, Code::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn sleeve_report_reads_bounded_summary_and_dispatches_over_http() {
    let home = tempfile::tempdir().unwrap();
    let summary = home.path().join("summary.txt");
    std::fs::write(&summary, "Built the feature and ran the tests.").unwrap();
    let (addr, server, _, chat, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let path = summary.to_string_lossy().into_owned();
    let output = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &base,
            OWNER,
            ["report", "--status", "complete", "--summary-file", &path],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output, "reported complete at chat seq 13\n");
    assert_eq!(
        chat.report.lock().unwrap().as_slice(),
        &[(
            "REPORT_STATUS_COMPLETE".to_owned(),
            "Built the feature and ran the tests.".to_owned()
        )]
    );
    server.abort();

    assert!(parse(["report", "--status", "partial", "-"]).is_ok());
    assert!(parse(["report", "--status", "blocked", "--summary-file", "-"]).is_ok());
    assert!(parse(["report", "--status", "complete", "--summary-file", "a", "-"]).is_err());
    assert!(parse(["report", "--status", "other"]).is_err());
    assert!(parse(["report"]).is_err());
    std::fs::write(&summary, vec![b'x'; 32 * 1024 + 1]).unwrap();
    assert_eq!(
        protectorate_cli::dispatch_endpoint(
            "http://127.0.0.1:1",
            OWNER,
            [
                "report",
                "--status",
                "complete",
                "--summary-file",
                summary.to_str().unwrap()
            ]
        )
        .unwrap_err()
        .code,
        Code::InvalidArgument
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_send_and_wait_dispatch_over_http() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, chat, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let (calls, sends, waits) = tokio::task::spawn_blocking(move || {
        let commands: Vec<Vec<&str>> = vec![
            vec!["chat", "send", "sleeve-x", "fix", "the", "hub"],
            vec![
                "--json",
                "chat",
                "send",
                "sleeve-x",
                "hello",
                "--request-id=retry-1",
            ],
            vec!["chat", "send", "exited-x", "hello"],
            vec!["chat", "send", "ghost", "hello"],
            vec!["chat", "wait", "sleeve-x"],
            vec!["--json", "chat", "wait", "reported-x", "--until", "report"],
            vec!["chat", "wait", "busy-x", "--timeout", "250ms"],
            vec![
                "chat",
                "wait",
                "exited-x",
                "--until=report",
                "--timeout",
                "1s",
            ],
            vec!["chat", "wait", "ghost"],
            vec![
                "--json",
                "chat",
                "wait",
                "sleeve-x",
                "--until",
                "idle",
                "--timeout",
                "1m30s",
            ],
        ];
        let calls = commands
            .iter()
            .map(|args| code(dispatch_endpoint(&base, OWNER, args.iter())))
            .collect::<Vec<_>>();
        let sends = chat.send.lock().unwrap().clone();
        let waits = chat.wait.lock().unwrap().clone();
        (calls, sends, waits)
    })
    .await
    .unwrap();
    server.abort();

    let mut calls = calls.into_iter();

    assert_eq!(ok(&mut calls, "chat send"), "seq\t7\n");

    let sent: Value = serde_json::from_str(&ok(&mut calls, "json chat send")).unwrap();
    assert_eq!(sent["seq"], "7", "{sent}");

    assert_eq!(
        fail(&mut calls, "chat send to an exited sleeve"),
        Code::FailedPrecondition
    );
    assert_eq!(fail(&mut calls, "chat send to no sleeve"), Code::NotFound);

    assert_eq!(ok(&mut calls, "chat wait"), "sleeve-x\tidle\n");

    let waited: Value =
        serde_json::from_str(&ok(&mut calls, "json chat wait --until report")).unwrap();
    assert_eq!(waited["sleeve"], "reported-x");
    assert_eq!(waited["result"]["status"], "REPORT_STATUS_COMPLETE");
    assert_eq!(waited["result"]["summary"], "tests pass");
    assert_eq!(waited["result"]["chatSeq"], "9");

    assert_eq!(
        fail(&mut calls, "chat wait on a busy sleeve"),
        Code::DeadlineExceeded
    );
    assert_eq!(
        fail(&mut calls, "chat wait on an exited sleeve"),
        Code::FailedPrecondition
    );
    assert_eq!(fail(&mut calls, "chat wait on no sleeve"), Code::NotFound);

    let waited: Value = serde_json::from_str(&ok(&mut calls, "json chat wait")).unwrap();
    assert_eq!(waited["sleeve"], "sleeve-x");
    assert!(waited.get("result").is_none(), "{waited}");

    // What the hub was actually asked, in protobuf JSON field names.
    assert_eq!(
        sends,
        vec![
            (
                "sleeve-x".to_owned(),
                "fix the hub".to_owned(),
                String::new()
            ),
            (
                "sleeve-x".to_owned(),
                "hello".to_owned(),
                "retry-1".to_owned()
            ),
        ],
        "{sends:?}"
    );
    assert_eq!(
        waits,
        vec![
            (
                "sleeve-x".to_owned(),
                "WAIT_UNTIL_IDLE".to_owned(),
                "0s".to_owned()
            ),
            (
                "reported-x".to_owned(),
                "WAIT_UNTIL_REPORT".to_owned(),
                "0s".to_owned()
            ),
            (
                "busy-x".to_owned(),
                "WAIT_UNTIL_IDLE".to_owned(),
                "0.250s".to_owned()
            ),
            (
                "exited-x".to_owned(),
                "WAIT_UNTIL_REPORT".to_owned(),
                "1s".to_owned()
            ),
            (
                "ghost".to_owned(),
                "WAIT_UNTIL_IDLE".to_owned(),
                "0s".to_owned()
            ),
            (
                "sleeve-x".to_owned(),
                "WAIT_UNTIL_IDLE".to_owned(),
                "90s".to_owned()
            ),
        ],
        "{waits:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn backlog_add_done_and_reopen_write_a_real_connect_server() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, note_id, _, _) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let body_file = home.path().join("body.md");
    std::fs::write(&body_file, "body from a file\nsecond line\n").unwrap();
    let body_file = body_file.to_string_lossy().into_owned();
    let out = tokio::task::spawn_blocking(move || {
        let added = dispatch_endpoint(
            &base,
            OWNER,
            [
                "backlog",
                "add",
                "Ship the CLI notes",
                "--project",
                "protectorate",
                "--pin",
            ],
        )
        .expect("backlog add");
        let id = added
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let done = dispatch_endpoint(
            &base,
            OWNER,
            [
                "--json",
                "backlog",
                "done",
                id.as_str(),
                "--resolution",
                "shipped in v1",
            ],
        )
        .expect("backlog done");
        let reopened = dispatch_endpoint(&base, OWNER, ["backlog", "reopen", id.as_str()])
            .expect("backlog reopen");
        let from_file = dispatch_endpoint(
            &base,
            OWNER,
            [
                "--json",
                "backlog",
                "add",
                "from a file",
                "--body-file",
                body_file.as_str(),
            ],
        )
        .expect("backlog add --body-file");
        let from_body = dispatch_endpoint(
            &base,
            OWNER,
            ["backlog", "add", "--body", "body only, no title"],
        )
        .expect("backlog add --body");
        let empty = code(dispatch_endpoint(&base, OWNER, ["backlog", "add"]));
        let missing_done = code(dispatch_endpoint(
            &base,
            OWNER,
            ["backlog", "done", "no-such-note"],
        ));
        let missing_reopen = code(dispatch_endpoint(
            &base,
            OWNER,
            ["backlog", "reopen", "no-such-note"],
        ));
        let all =
            dispatch_endpoint(&base, OWNER, ["backlog", "ls", "--all"]).expect("backlog ls --all");
        (
            added,
            id,
            done,
            reopened,
            from_file,
            from_body,
            empty,
            missing_done,
            missing_reopen,
            all,
        )
    })
    .await
    .unwrap();
    server.abort();
    let (added, id, done, reopened, from_file, from_body, empty, missing_done, missing_reopen, all) =
        out;

    assert!(!id.is_empty(), "{added}");
    assert!(added.contains("open"), "{added}");
    assert!(added.contains("pin"), "{added}");
    assert!(added.contains("protectorate"), "{added}");
    assert!(added.contains("Ship the CLI notes"), "{added}");

    let done: Value = serde_json::from_str(&done).unwrap();
    assert_eq!(done["id"], id);
    assert_eq!(done["state"], "done");
    assert_eq!(done["completion"]["resolution"], "shipped in v1");
    assert_eq!(
        done["completion"]["doneBy"], "owner",
        "the hub names the actor"
    );
    assert!(done["completion"]["doneAt"].is_string(), "{done}");

    assert!(reopened.contains("open"), "{reopened}");
    assert!(
        !reopened.contains("done"),
        "reopen clears the completion: {reopened}"
    );

    let from_file: Value = serde_json::from_str(&from_file).unwrap();
    assert_eq!(from_file["title"], "from a file");
    assert_eq!(from_file["body"], "body from a file\nsecond line\n");
    assert_eq!(
        from_file["source"], "owner",
        "the hub takes the source from the token"
    );

    assert!(from_body.contains("body only, no title"), "{from_body}");

    assert_eq!(
        empty.unwrap_err(),
        Code::InvalidArgument,
        "a note needs a title or a body"
    );
    assert_eq!(missing_done.unwrap_err(), Code::NotFound);
    assert_eq!(missing_reopen.unwrap_err(), Code::NotFound);

    assert!(all.contains("Ship the CLI notes"), "{all}");
    assert!(all.contains("from a file"), "{all}");
    assert!(all.contains("body only, no title"), "{all}");
    assert!(all.contains(note_id.as_str()), "{all}");
}

#[test]
fn backlog_usage_lists_the_new_commands() {
    let help = run(["--help"]).unwrap();
    assert!(help.contains("backlog add [title]"), "{help}");
    assert!(help.contains("backlog done <id>"), "{help}");
    assert!(help.contains("backlog reopen <id>"), "{help}");
}

#[test]
fn chat_wait_extends_the_transport_past_its_own_deadline() {
    let invocation = parse(["chat", "wait", "sleeve-x", "--timeout", "5m"]).unwrap();
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(305));

    // A wait with no hub deadline still has a transport ceiling.
    let invocation = parse(["chat", "wait", "sleeve-x"]).unwrap();
    assert_eq!(
        transport_timeout(&invocation),
        Duration::from_secs(24 * 3600),
        "chat wait --timeout 0 blocks"
    );

    let invocation = parse(["--timeout", "30", "job", "ls"]).unwrap();
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(30));
}

#[test]
fn chat_usage_lists_the_new_commands() {
    let help = run(["--help"]).unwrap();
    assert!(help.contains("chat send <sleeve> <text>"), "{help}");
    assert!(help.contains("chat wait <sleeve>"), "{help}");
    assert!(help.contains("chat watch <sleeve>"), "{help}");
    assert!(help.contains("chat respond <sleeve> <id>"), "{help}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sleeve_lifecycle_commands_send_their_options_over_http() {
    let home = tempfile::tempdir().unwrap();
    let workspace = home.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let brief = home.path().join("brief.txt");
    std::fs::write(&brief, "Review the change.\n").unwrap();
    let (addr, server, _, _, sleeve_calls) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let workspace = workspace.to_str().unwrap().to_owned();
    let workspace_expected = workspace.clone();
    let brief = brief.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        let spawn = dispatch_endpoint(
            &base,
            OWNER,
            [
                "sleeve",
                "spawn",
                "worker",
                "--workspace",
                &workspace,
                "--harness",
                "codex",
                "--mode",
                "chat",
                "--cpu",
                "200",
                "--mem",
                "512M",
                "--env",
                "A=one=two",
                "--ro",
                "/host/docs:/docs",
                "--branch",
                "main",
                "--new-branch",
                "task",
                "--brief",
                &brief,
                "--exit-after-idle",
                "5m",
                "--model",
                "gpt-test",
                "--effort",
                "high",
                "--supervised",
                "--nested",
                "--",
                "--extra",
            ],
        )
        .unwrap();
        let envoy = dispatch_endpoint(
            &base,
            OWNER,
            [
                "sleeve",
                "envoy",
                "--harness",
                "codex",
                "--scopes",
                "message_*,lease_*",
                "--add-scopes",
                "backlog_*",
                "--ttl",
                "1h30m",
                "--model",
                "gpt-test",
            ],
        )
        .unwrap();
        let resleeve = dispatch_endpoint(
            &base,
            OWNER,
            [
                "sleeve",
                "resleeve",
                "worker",
                "--to",
                "pi",
                "--interrupt",
                "--wait",
                "7m",
                "--native-defaults",
            ],
        )
        .unwrap();
        let prune = dispatch_endpoint(
            &base,
            OWNER,
            ["sleeve", "prune", "--workspaces", "--ignored", "--running"],
        )
        .unwrap();
        (spawn, envoy, resleeve, prune)
    })
    .await
    .unwrap();
    assert!(output.0.contains("spawned"), "{}", output.0);
    assert!(
        output.0.contains("protectorate chat send worker"),
        "{}",
        output.0
    );
    assert!(output.1.contains("message_*"), "{}", output.1);
    assert!(output.2.contains("resleeved"), "{}", output.2);
    assert!(output.3.contains("Dry run"), "{}", output.3);
    let calls = sleeve_calls.lifecycle.lock().unwrap().clone();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0].0, "SpawnSleeve");
    assert_eq!(calls[0].1["workspace"], workspace_expected);
    assert_eq!(calls[0].1["harness"], "HARNESS_CODEX");
    assert_eq!(calls[0].1["args"][0], "--extra");
    assert_eq!(calls[0].1["limits"]["cpuPercent"], 200);
    assert_eq!(calls[0].1["limits"]["memoryBytes"], "536870912");
    assert_eq!(calls[0].1["env"]["A"], "one=two");
    assert_eq!(calls[0].1["readOnly"][0]["target"], "/docs");
    assert_eq!(calls[0].1["brief"], "Review the change.\n");
    assert_eq!(calls[0].1["exitAfterIdle"], "300s");
    assert_eq!(calls[0].1["supervised"], true);
    assert_eq!(calls[0].1["nested"], true);
    assert_eq!(calls[0].1["selection"]["model"], "gpt-test");
    assert_eq!(calls[1].0, "SpawnEnvoy");
    assert_eq!(calls[1].1["ttl"], "5400s");
    assert_eq!(calls[1].1["scopes"], json!(["message_*", "lease_*"]));
    assert_eq!(calls[1].1["addScopes"], json!(["backlog_*"]));
    assert_eq!(calls[2].0, "ResleeveSleeve");
    assert_eq!(calls[2].1["to"], "HARNESS_PI");
    assert_eq!(calls[2].1["wait"], "420s");
    assert_eq!(calls[2].1["interrupt"], true);
    assert_eq!(calls[2].1["nativeDefaults"], true);
    assert_eq!(calls[3].0, "PruneSleeves");
    assert_eq!(calls[3].1["workspaces"], true);
    assert_eq!(calls[3].1["ignored"], true);
    assert_eq!(calls[3].1["running"], true);
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn sleeve_commands_read_a_real_connect_server() {
    let home = tempfile::tempdir().unwrap();
    let (addr, server, _, _, sleeve_calls) = serve(home.path(), false).await;
    let base = format!("http://{addr}");
    let (calls, kills, panes) = tokio::task::spawn_blocking(move || {
        let commands: Vec<Vec<&str>> = vec![
            vec!["sleeve", "ls"],
            vec!["--json", "sleeve", "list"],
            vec!["sleeve", "show", "running-x"],
            vec!["sleeve", "show", "exited-x"],
            vec!["sleeve", "show", "oom-x"],
            vec!["--json", "sleeve", "show", "killed-x"],
            vec!["sleeve", "show", "ghost"],
            vec!["sleeve", "kill", "running-x"],
            vec!["--json", "sleeve", "kill", "exited-x", "--drop-caches"],
            vec!["sleeve", "kill", "ghost"],
        ];
        let calls = commands
            .iter()
            .map(|args| code(dispatch_endpoint(&base, OWNER, args.iter())))
            .collect::<Vec<_>>();
        (
            calls,
            sleeve_calls.kills.lock().unwrap().clone(),
            sleeve_calls.panes.lock().unwrap().clone(),
        )
    })
    .await
    .unwrap();
    server.abort();

    let mut calls = calls.into_iter();

    let list = ok(&mut calls, "sleeve ls");
    let rows: Vec<&str> = list.lines().collect();
    assert_eq!(rows.len(), 5, "{list}");
    assert!(rows[0].starts_with("NAME"), "{list}");
    assert!(rows[1].starts_with("running-x"), "{}", rows[1]);
    assert!(rows[1].contains("pi"), "{}", rows[1]);
    assert!(rows[1].contains("gpt-5.2-codex"), "{}", rows[1]);
    assert!(rows[1].contains("high"), "{}", rows[1]);
    assert!(rows[1].contains("running"), "{}", rows[1]);
    assert!(rows[1].contains("12%"), "{}", rows[1]);
    assert!(rows[1].contains("1.0MiB"), "{}", rows[1]);
    assert!(rows[1].contains("2.0MiB"), "{}", rows[1]);
    assert!(rows[1].contains("3.0GiB"), "{}", rows[1]);
    assert!(rows[1].contains("200%/8.0GiB"), "{}", rows[1]);
    assert!(rows[1].contains("/home/u/github/app"), "{}", rows[1]);
    assert!(rows[1].contains("owner"), "{}", rows[1]);
    assert!(rows[1].contains("main"), "{}", rows[1]);
    assert!(rows[2].starts_with("exited-x"), "{}", rows[2]);
    assert!(rows[2].contains("shell"), "{}", rows[2]);
    assert!(rows[2].contains("exited(3)"), "{}", rows[2]);
    assert!(rows[3].starts_with("killed-x"), "{}", rows[3]);
    assert!(rows[3].contains("killed"), "{}", rows[3]);
    assert!(rows[4].starts_with("oom-x"), "{}", rows[4]);
    assert!(rows[4].contains("exited(137,oom)"), "{}", rows[4]);
    assert!(rows[4].contains("unknown"), "{}", rows[4]);

    let sleeves: Vec<Value> = ok(&mut calls, "json sleeve list")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(sleeves.len(), 4, "{sleeves:?}");
    assert_eq!(sleeves[0]["name"], "running-x", "{sleeves:?}");
    assert_eq!(sleeves[0]["state"], "SLEEVE_STATE_RUNNING", "{sleeves:?}");

    let shown = ok(&mut calls, "sleeve show running-x");
    assert!(shown.contains("harness"), "{shown}");
    assert!(shown.contains("pi"), "{shown}");
    assert!(shown.contains("mode"), "{shown}");
    assert!(shown.contains("chat"), "{shown}");
    assert!(shown.contains("yes (the harness asks"), "{shown}");
    assert!(shown.contains("complete (chat seq 9"), "{shown}");
    assert!(shown.contains("tests pass"), "{shown}");
    assert!(shown.contains("codex (session ses_0) to pi at"), "{shown}");
    assert!(shown.contains("by claude anthropic claude-opus"), "{shown}");
    assert!(shown.contains("/etc/ssl -> /etc/ssl"), "{shown}");
    assert!(shown.contains("12000/272000"), "{shown}");
    assert!(shown.contains("12.4% (30.4s total)"), "{shown}");
    assert!(shown.contains("1.0MiB (peak 2.0MiB)"), "{shown}");
    assert!(
        !shown.contains("last 20 lines of output"),
        "a running sleeve has no pane tail: {shown}"
    );

    let shown = ok(&mut calls, "sleeve show exited-x");
    assert!(shown.contains("exited(3)"), "{shown}");
    assert!(shown.contains("exit 3"), "{shown}");
    assert!(shown.contains("exit signal"), "{shown}");
    assert!(shown.contains("sh -c exit 3"), "{shown}");
    assert!(shown.contains("last 20 lines of output:"), "{shown}");
    assert!(shown.contains("output line 6    trailing"), "{shown}");
    assert!(shown.contains("output line 25    trailing"), "{shown}");
    assert!(
        !shown.contains("output line 5"),
        "the tail keeps the last 20 lines: {shown}"
    );
    assert!(
        !shown.contains("\u{1b}[31m"),
        "SGR escapes are stripped: {shown}"
    );

    let shown = ok(&mut calls, "sleeve show oom-x");
    assert!(shown.contains("exit 137 (OOM killed)"), "{shown}");
    assert!(
        shown.contains("(no output kept: the session is gone)"),
        "{shown}"
    );

    let sleeve: Value = serde_json::from_str(&ok(&mut calls, "json sleeve show")).unwrap();
    assert_eq!(sleeve["sleeve"]["name"], "killed-x");
    assert_eq!(sleeve["sleeve"]["killed"], true);
    assert_eq!(sleeve["sleeve"]["exitCode"], 137);
    assert_eq!(sleeve["sleeve"]["exitSignal"], 9);

    assert_eq!(fail(&mut calls, "sleeve show ghost"), Code::NotFound);

    assert_eq!(
        ok(&mut calls, "sleeve kill"),
        "killed\trunning-x\n",
        "the Go CLI's kill line"
    );

    let killed: Value =
        serde_json::from_str(&ok(&mut calls, "json sleeve kill --drop-caches")).unwrap();
    assert_eq!(killed, json!({}), "KillSleeveResponse is empty");

    assert_eq!(fail(&mut calls, "sleeve kill ghost"), Code::NotFound);

    // What the hub was actually asked, in protobuf JSON field names.
    assert_eq!(
        kills,
        vec![
            ("running-x".to_owned(), false),
            ("exited-x".to_owned(), true),
        ],
        "{kills:?}"
    );
    assert_eq!(
        panes,
        vec![
            ("sleeve-exited-x".to_owned(), 100),
            ("sleeve-oom-x".to_owned(), 100),
            ("sleeve-killed-x".to_owned(), 100),
        ],
        "{panes:?}"
    );
}

#[test]
fn sleeve_usage_lists_the_new_commands() {
    let help = run(["--help"]).unwrap();
    assert!(help.contains("sleeve ls | sleeve list"), "{help}");
    assert!(help.contains("sleeve workspaces | ws"), "{help}");
    assert!(help.contains("sleeve browse [path]"), "{help}");
    assert!(help.contains("sleeve show <name>"), "{help}");
    assert!(help.contains("sleeve kill <name>"), "{help}");
    assert!(help.contains("sleeve attach <name>"), "{help}");
    assert!(help.contains("sleeve spawn <name>"), "{help}");
    assert!(help.contains("sleeve envoy"), "{help}");
    assert!(help.contains("sleeve resleeve <name>"), "{help}");
    assert!(help.contains("sleeve prune"), "{help}");
}
