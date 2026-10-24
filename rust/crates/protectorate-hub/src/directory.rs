//! Directory RPC projection from the persistent workspace graph.
use super::{Hub, NewNote};
use protectorate_backlog::Filter;
use protectorate_directory::{self as directory, Change, Edge, Facts, View};
use protectorate_sleeve_registry::State;
use protectorate_wire::{
    v1::{self, directory_service},
    Call, Code, Result, RpcError, UnaryServer,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

const SCAN_INTERVAL: Duration = Duration::from_secs(60);

fn refresh(hub: &Hub) -> directory::Result<()> {
    let scans = hub
        .sleeves
        .list()
        .into_iter()
        .map(|row| super::sleeves::refresh_record(hub, &row))
        .filter(|row| row.state == State::Running && !row.helper)
        .map(|row| directory::Scan {
            sleeve: row.name,
            workspace: directory::inspect(row.workspace),
        })
        .collect::<Vec<_>>();
    if let Err(error) = hub.directory.update(&scans) {
        eprintln!("directory: update projects: {error}");
    }
    check_releases(hub);
    Ok(())
}

fn check_releases(hub: &Hub) {
    for view in hub.directory.list() {
        let Some((workspace, now)) = view.project.owners.iter().find_map(|owner| {
            directory::read_refs(&owner.path)
                .ok()
                .map(|refs| (owner.path.clone(), refs))
        }) else {
            continue;
        };
        let Some(old) = &view.project.refs else {
            if let Err(error) = hub.directory.set_refs(&view.project.id, now) {
                eprintln!("directory: baseline refs of {}: {error}", view.project.id);
            }
            continue;
        };
        let ancestor = |a: &str, b: &str| directory::is_ancestor(&workspace, a, b);
        for change in directory::changes(old, &now, ancestor) {
            announce_release(hub, &view, &workspace, &change);
        }
        let merged = directory::merge(old, &now, ancestor);
        if &merged != old {
            if let Err(error) = hub.directory.set_refs(&view.project.id, merged) {
                eprintln!("directory: save refs of {}: {error}", view.project.id);
            }
        }
    }
}

fn announce_release(hub: &Hub, upstream: &View, workspace: &Path, change: &Change) {
    let facts = directory::describe(workspace, &change.from, &change.to).unwrap_or_else(|error| {
        eprintln!(
            "directory: describe {} {}: {error}",
            upstream.project.id, change.name
        );
        Facts::default()
    });
    hub.emit(
        "project.released",
        &upstream.project.id,
        json!({
            "kind": change.kind, "name": change.name, "from": change.from,
            "to": change.to, "dependents": upstream.dependents.len(),
        }),
    );
    let reference = if change.tag.is_empty() {
        &change.to
    } else {
        &change.tag
    };
    let key = format!("{}:{reference}:{}", change.kind, change.to);
    let reply_via = project_address(hub, upstream);
    for edge in &upstream.dependents {
        let Ok(downstream) = hub.directory.get(&edge.project) else {
            continue;
        };
        let requests = hub
            .mail
            .open_requests(&upstream.project.id, &downstream.project.id);
        let body = release_body(upstream, &downstream, change, &edge.via, &facts, &requests);
        let active = downstream.project.owners.iter().any(|owner| {
            hub.sleeves
                .get(&owner.sleeve)
                .is_ok_and(|row| row.state == State::Running)
        });
        if active {
            let id = dep_request_id(&upstream.project.id, &key, &downstream.project.id);
            if let Err(error) = super::mail::send_release_notice(
                hub,
                &format!("project:{}", downstream.project.id),
                body,
                id,
                reply_via.clone(),
            ) {
                eprintln!(
                    "directory: notify {} of {key}: {error}",
                    downstream.project.id
                );
            }
        } else {
            file_release(hub, upstream, &downstream, change, &edge.via, body);
        }
    }
}

fn project_address(hub: &Hub, project: &View) -> String {
    let display = &project.project.display;
    let name = hub
        .directory
        .get(display)
        .ok()
        .filter(|view| view.project.id == project.project.id)
        .map_or(project.project.id.as_str(), |_| display.as_str());
    format!("project:{name}")
}

fn dep_request_id(upstream: &str, key: &str, downstream: &str) -> String {
    let digest = Sha256::digest(format!("{upstream}\0{key}\0{downstream}").as_bytes());
    format!("dep-{}", &format!("{digest:x}")[..32])
}

fn release_headline(change: &Change) -> String {
    if change.kind == "tag" {
        format!("released {}", change.tag)
    } else {
        format!(
            "default branch {} advanced to {}",
            change.name,
            directory::short(&change.to)
        )
    }
}

fn file_release(
    hub: &Hub,
    upstream: &View,
    downstream: &View,
    change: &Change,
    via: &[String],
    body: String,
) {
    let title = format!(
        "{} {}: review changes (uses {})",
        upstream.project.display,
        release_headline(change),
        via.join(", ")
    );
    let existing = hub.backlog.list(Filter {
        project: downstream.project.display.clone(),
        state: "all".into(),
        ..Default::default()
    });
    match existing {
        Ok(notes) if notes.iter().any(|note| note.title == title) => return,
        Err(error) => {
            eprintln!("directory: read release notes: {error}");
            return;
        }
        _ => {}
    }
    match hub.backlog.create(NewNote {
        title,
        body,
        project: downstream.project.display.clone(),
        source: "hub".into(),
        ..Default::default()
    }) {
        Ok(note) => {
            hub.emit("note.created", &note.id, super::note_event_payload(&note));
        }
        Err(error) => eprintln!(
            "directory: file release note for {}: {error}",
            downstream.project.id
        ),
    }
}

fn release_body(
    upstream: &View,
    downstream: &View,
    change: &Change,
    via: &[String],
    facts: &Facts,
    requests: &[protectorate_mailbox::Message],
) -> String {
    let mut body = if change.kind == "tag" {
        let mut line = format!(
            "Upstream release: {} tag {} ({})",
            upstream.project.display,
            change.tag,
            directory::short(&change.to)
        );
        if !change.from_name.is_empty() {
            line.push_str(&format!(
                ", previous tag {} ({})",
                change.from_name,
                directory::short(&change.from)
            ));
        } else if !change.from.is_empty() {
            line.push_str(&format!(
                ", previous default branch tip {}",
                directory::short(&change.from)
            ));
        }
        line
    } else {
        format!(
            "Upstream change: {} default branch {} advanced {} -> {}",
            upstream.project.display,
            change.name,
            directory::short(&change.from),
            directory::short(&change.to)
        )
    };
    body.push_str(&format!(
        ".\n{} depends on it through: {}.",
        downstream.project.display,
        via.join(", ")
    ));
    let facts = facts.to_string();
    if !facts.is_empty() {
        body.push_str(&format!("\n\n{facts}"));
    }
    if !requests.is_empty() {
        body.push_str(&format!(
            "\nYour open requests to {}:\n",
            upstream.project.display
        ));
        for request in requests {
            let line = request.body.lines().next().unwrap_or("");
            let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
            let excerpt = if normalized.chars().count() > 100 {
                format!("{}...", normalized.chars().take(99).collect::<String>())
            } else {
                normalized
            };
            body.push_str(&format!(
                "  thread {}, {}: {}\n",
                request.thread, request.id, excerpt
            ));
        }
        body.push_str("Close one with: protectorate msg ack <id>\n");
    }
    body.trim_end_matches('\n').to_owned()
}

fn start_scanner(hub: &Arc<Hub>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let weak = Arc::downgrade(hub);
    let kick = Arc::clone(&hub.directory_kick);
    let has_running = hub
        .sleeves
        .list()
        .iter()
        .any(|row| row.state == State::Running && !row.helper);
    runtime.spawn(async move {
        let mut interval =
            tokio::time::interval_at(tokio::time::Instant::now() + SCAN_INTERVAL, SCAN_INTERVAL);
        if has_running {
            kick.notify_one();
        }
        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = kick.notified() => {},
            }
            let Some(hub) = weak.upgrade() else { break };
            match tokio::task::spawn_blocking(move || refresh(&hub)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("directory scan failed: {error}"),
                Err(error) => eprintln!("directory scan worker failed: {error}"),
            }
        }
    });
}

fn error(error: directory::Error) -> RpcError {
    let code = match error {
        directory::Error::NotFound(_) => Code::NotFound,
        directory::Error::Ambiguous { .. } => Code::InvalidArgument,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn names(views: &[View]) -> BTreeMap<&str, &str> {
    views
        .iter()
        .map(|v| (v.project.id.as_str(), v.project.display.as_str()))
        .collect()
}

fn edge(edge: &Edge, names: &BTreeMap<&str, &str>) -> v1::ProjectEdge {
    v1::ProjectEdge {
        project: edge.project.clone(),
        display: names
            .get(edge.project.as_str())
            .copied()
            .unwrap_or("")
            .into(),
        via: edge.via.clone(),
    }
}

fn project(view: &View, names: &BTreeMap<&str, &str>) -> v1::DirectoryProject {
    let p = &view.project;
    v1::DirectoryProject {
        id: p.id.clone(),
        name: p.name.clone(),
        display: p.display.clone(),
        remote: p.remote.clone(),
        owners: p
            .owners
            .iter()
            .map(|owner| v1::ProjectOwner {
                sleeve: owner.sleeve.clone(),
                workspace: owner.path.to_string_lossy().into_owned(),
                branch: owner.branch.clone(),
                head: owner.head.clone(),
            })
            .collect(),
        provides: p.provides.clone(),
        dependency_count: p.deps.len().try_into().unwrap_or(u32::MAX),
        depends_on: view.depends_on.iter().map(|e| edge(e, names)).collect(),
        dependents: view.dependents.iter().map(|e| edge(e, names)).collect(),
        default_branch: p
            .refs
            .as_ref()
            .map(|refs| refs.default.clone())
            .unwrap_or_default(),
        latest_tag: p
            .refs
            .as_ref()
            .map(|refs| refs.latest_tag().to_owned())
            .unwrap_or_default(),
    }
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    start_scanner(&hub);
    let list = Arc::clone(&hub);
    server.unary(
        directory_service::LIST_PROJECTS,
        move |call: Call<v1::ListProjectsRequest>| {
            let hub = Arc::clone(&list);
            async move {
                let sleeve = call.request.sleeve;
                let views = hub.directory.list();
                let names = names(&views);
                Ok(v1::ListProjectsResponse {
                    projects: views
                        .iter()
                        .filter(|v| {
                            sleeve.is_empty() || v.project.owners.iter().any(|o| o.sleeve == sleeve)
                        })
                        .map(|v| project(v, &names))
                        .collect(),
                })
            }
        },
    )?;
    server.unary(
        directory_service::GET_PROJECT,
        move |call: Call<v1::GetProjectRequest>| {
            let hub = Arc::clone(&hub);
            async move {
                let view = hub.directory.get(&call.request.project).map_err(error)?;
                let views = hub.directory.list();
                Ok(v1::GetProjectResponse {
                    project: Some(project(&view, &names(&views))),
                })
            }
        },
    )?;
    Ok(())
}
