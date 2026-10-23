//! Directory RPC projection from the persistent workspace graph.
use super::{input, respond, string, Hub, NewNote};
use protectorate_backlog::Filter;
use protectorate_directory::{self as directory, Change, Edge, Facts, View};
use protectorate_sleeve_registry::State;
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
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
            if let Some(events) = &hub.note_events {
                events("note.created", &note);
            }
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

fn edge_json(edge: &Edge, names: &BTreeMap<&str, &str>) -> Value {
    json!({
        "project": edge.project,
        "display": names.get(edge.project.as_str()).copied().unwrap_or(""),
        "via": edge.via,
    })
}

fn project_json(view: &View, names: &BTreeMap<&str, &str>) -> Value {
    let p = &view.project;
    let mut out = json!({
        "id": p.id, "name": p.name, "display": p.display, "remote": p.remote,
        "owners": p.owners.iter().map(|owner| json!({
            "sleeve": owner.sleeve,
            "workspace": owner.path.to_string_lossy(),
            "branch": owner.branch,
            "head": owner.head,
        })).collect::<Vec<_>>(),
        "provides": p.provides,
        "dependencyCount": p.deps.len(),
        "dependsOn": view.depends_on.iter().map(|e| edge_json(e, names)).collect::<Vec<_>>(),
        "dependents": view.dependents.iter().map(|e| edge_json(e, names)).collect::<Vec<_>>(),
    });
    if let Some(refs) = &p.refs {
        out["defaultBranch"] = json!(refs.default);
        out["latestTag"] = json!(refs.latest_tag());
    }
    out
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    start_scanner(&hub);
    let list = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.DirectoryService/ListProjects",
        move |request| {
            let hub = Arc::clone(&list);
            async move {
                let sleeve = string(&input(&request)?, "sleeve");
                let views = hub.directory.list();
                let names = names(&views);
                let projects = views
                    .iter()
                    .filter(|v| {
                        sleeve.is_empty() || v.project.owners.iter().any(|o| o.sleeve == sleeve)
                    })
                    .map(|v| project_json(v, &names))
                    .collect::<Vec<_>>();
                respond(&request, json!({"projects": projects}))
            }
        },
    )?;
    server.register(
        "/protectorate.v1.DirectoryService/GetProject",
        move |request| {
            let hub = Arc::clone(&hub);
            async move {
                let project = string(&input(&request)?, "project");
                let view = hub.directory.get(&project).map_err(error)?;
                let views = hub.directory.list();
                respond(
                    &request,
                    json!({"project": project_json(&view, &names(&views))}),
                )
            }
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use protectorate_sleeve_registry::{Harness, Mode, Record};
    use std::{fs, path::Path, process::Command};

    fn git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repo(path: &Path, name: &str, dependencies: &str) {
        fs::create_dir_all(path).unwrap();
        git(path, &["init", "-q", "-b", "main"]);
        git(path, &["config", "user.email", "test@example.test"]);
        git(path, &["config", "user.name", "Test"]);
        git(
            path,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://github.com/acme/{name}.git"),
            ],
        );
        fs::write(
            path.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{dependencies}"),
        )
        .unwrap();
        git(path, &["add", "Cargo.toml"]);
        git(path, &["commit", "-qm", "initial"]);
    }

    #[test]
    fn releases_notify_live_dependents_and_file_notes_for_idle_ones() {
        let temp = tempfile::tempdir().unwrap();
        let upstream = temp.path().join("lib");
        let downstream = temp.path().join("app");
        repo(&upstream, "lib", "");
        repo(&downstream, "app", "[dependencies]\nlib = \"1\"\n");
        git(&upstream, &["tag", "v0.1.0"]);
        let home = temp.path().join("state");
        let hub = Hub::open(&home, "owner-secret", "test").unwrap();
        for (name, path) in [("lib-worker", &upstream), ("app-worker", &downstream)] {
            hub.sleeves
                .record_spawned(
                    Record::new(name, Harness::Pi, Mode::Chat, path, "2026-01-01T00:00:00Z")
                        .unwrap(),
                )
                .unwrap();
        }
        refresh(&hub).unwrap();
        let baseline = hub.directory.get("acme/lib").unwrap();
        assert_eq!(baseline.project.refs.as_ref().unwrap().default, "main");
        assert_eq!(
            baseline.project.refs.as_ref().unwrap().latest_tag(),
            "v0.1.0"
        );
        assert_eq!(baseline.dependents[0].project, "github.com/acme/app");
        assert!(hub
            .mail
            .inbox("app-worker", "", false, true)
            .unwrap()
            .is_empty());

        hub.mail
            .send(protectorate_mailbox::NewMessage {
                from: "app-worker".into(),
                to: "project:acme/lib".into(),
                kind: protectorate_mailbox::Kind::REQUEST,
                ack_required: true,
                body: "Please keep the API stable".into(),
                from_project: "github.com/acme/app".into(),
                project: "github.com/acme/lib".into(),
                recipients: vec!["lib-worker".into()],
                ..Default::default()
            })
            .unwrap();
        fs::write(upstream.join("CHANGELOG.md"), "second release\n").unwrap();
        git(&upstream, &["add", "CHANGELOG.md"]);
        git(&upstream, &["commit", "-qm", "second release"]);
        git(&upstream, &["tag", "v0.2.0"]);
        refresh(&hub).unwrap();
        let notices = hub.mail.inbox("app-worker", "", false, true).unwrap();
        assert_eq!(notices.len(), 1);
        assert!(notices[0]
            .body
            .contains("Upstream release: acme/lib tag v0.2.0"));
        assert!(notices[0].body.contains("previous tag v0.1.0"));
        assert!(notices[0].body.contains("CHANGELOG.md"));
        assert!(notices[0].body.contains("Please keep the API stable"));
        assert!(notices[0]
            .body
            .contains("Reply: protectorate msg send project:acme/lib --thread"));
        assert_eq!(
            hub.directory
                .get("acme/lib")
                .unwrap()
                .project
                .refs
                .unwrap()
                .latest_tag(),
            "v0.2.0"
        );
        refresh(&hub).unwrap();
        assert_eq!(
            hub.mail.inbox("app-worker", "", false, true).unwrap().len(),
            1
        );

        hub.sleeves
            .record_exit(
                "app-worker",
                "2026-01-01T00:01:00Z".into(),
                Some(0),
                0,
                false,
                false,
            )
            .unwrap();
        fs::write(upstream.join("CHANGELOG.md"), "branch advance\n").unwrap();
        git(&upstream, &["add", "CHANGELOG.md"]);
        git(&upstream, &["commit", "-qm", "branch advance"]);
        refresh(&hub).unwrap();
        let notes = hub
            .backlog
            .list(Filter {
                project: "acme/app".into(),
                state: "all".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].title.contains("default branch main advanced"));
        refresh(&hub).unwrap();
        assert_eq!(
            hub.backlog
                .list(Filter {
                    project: "acme/app".into(),
                    state: "all".into(),
                    ..Default::default()
                })
                .unwrap()
                .len(),
            1
        );
        drop(hub);
        let reopened = Hub::open(&home, "owner-secret", "test").unwrap();
        check_releases(&reopened);
        assert_eq!(
            reopened
                .mail
                .inbox("app-worker", "", false, true)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            reopened
                .backlog
                .list(Filter {
                    project: "acme/app".into(),
                    state: "all".into(),
                    ..Default::default()
                })
                .unwrap()
                .len(),
            1
        );
    }
}
