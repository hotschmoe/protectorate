//! Authenticated terminal WebSocket bridge to tmux's private PTY client.
use crate::{audit, now_unix, Hub};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path,
    },
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use std::{
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::unix::AsyncFd;

const PROTOCOL: &str = "protectorate.v1";
const MAX_FRAME: usize = 64 << 10;

pub(super) async fn attach(
    hub: Arc<Hub>,
    Path(name): Path<String>,
    tls: bool,
    headers: HeaderMap,
    uri: Uri,
    upgrade: WebSocketUpgrade,
) -> Response {
    let query = uri.query().unwrap_or("");
    let read_only = query_value(query, "readonly") == Some("1");
    let cols = query_value(query, "cols")
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(80);
    let rows = query_value(query, "rows")
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(24);
    let caller = crate::access::authenticate(
        &hub.owner_token,
        &hub.identity,
        &hub.methods,
        &headers,
        "GET",
        uri.path(),
        tls,
    );
    let Ok(caller) = caller else {
        audit::record_terminal(
            &hub.audit,
            "unauthenticated",
            "",
            if protectorate_session::valid_name(&name).is_ok() {
                &name
            } else {
                ""
            },
            read_only,
            ("denied", "unauthenticated"),
        );
        return (
            StatusCode::UNAUTHORIZED,
            "missing or invalid terminal credential",
        )
            .into_response();
    };
    let auth = crate::access::context(&caller);
    let scopes = auth.scopes.iter().map(String::as_str).collect::<Vec<_>>();
    let allowed = auth.owner
        || auth.device
        || (protectorate_auth::scopes_allow_terminal(&scopes, read_only)
            && (read_only || name.starts_with("sleeve-") || scopes.contains(&"terminal:host")));
    let (owner, actor, token_id) = (auth.owner, auth.actor, auth.token_id);
    if protectorate_session::valid_name(&name).is_err() {
        audit::record_terminal(
            &hub.audit,
            &actor,
            &token_id,
            "",
            read_only,
            ("error", "invalid_argument"),
        );
        return (StatusCode::BAD_REQUEST, "invalid session name").into_response();
    }
    if !allowed {
        audit::record_terminal(
            &hub.audit,
            &actor,
            &token_id,
            &name,
            read_only,
            ("denied", "permission_denied"),
        );
        return (StatusCode::FORBIDDEN, "terminal token lacks required scope").into_response();
    }
    let manager = Arc::clone(&hub.sessions);
    let lookup_name = name.clone();
    let prepared =
        tokio::task::spawn_blocking(move || manager.attach_command(&lookup_name, read_only)).await;
    let (command, session) = match prepared {
        Ok(Ok(value)) => value,
        Ok(Err(protectorate_session::Error::NotFound | protectorate_session::Error::NoServer)) => {
            audit::record_terminal(
                &hub.audit,
                &actor,
                &token_id,
                &name,
                read_only,
                ("error", "not_found"),
            );
            return (StatusCode::NOT_FOUND, "session not found").into_response();
        }
        Ok(Err(protectorate_session::Error::InvalidName)) => {
            audit::record_terminal(
                &hub.audit,
                &actor,
                &token_id,
                &name,
                read_only,
                ("error", "invalid_argument"),
            );
            return (StatusCode::BAD_REQUEST, "invalid session name").into_response();
        }
        _ => {
            audit::record_terminal(
                &hub.audit,
                &actor,
                &token_id,
                &name,
                read_only,
                ("error", "internal"),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "attach failed").into_response();
        }
    };
    upgrade
        .protocols([PROTOCOL])
        .on_upgrade(move |socket| async move {
            let _ = bridge(
                hub, socket, name, owner, actor, token_id, command, session, read_only, cols, rows,
            )
            .await;
        })
        .into_response()
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
}

fn open_pty(cols: u16, rows: u16) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master = -1;
    let mut slave = -1;
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &size,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((master, slave))
}

fn spawn_client(
    mut command: Command,
    cols: u16,
    rows: u16,
) -> io::Result<(AsyncFd<OwnedFd>, std::process::Child)> {
    let (master, slave) = open_pty(cols, rows)?;
    let stdin = slave.try_clone()?;
    let stdout = slave.try_clone()?;
    command
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    Ok((
        unsafe { AsyncFd::register(master) }.map_err(io::Error::from)?,
        child,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn bridge(
    hub: Arc<Hub>,
    mut socket: WebSocket,
    name: String,
    owner: bool,
    actor: String,
    token_id: String,
    command: Command,
    session: protectorate_session::Session,
    mut read_only: bool,
    cols: u16,
    rows: u16,
) -> io::Result<()> {
    let requested_read_only = read_only;
    if session.state == protectorate_session::State::Exited {
        read_only = true;
        let notice = exit_notice(&session);
        if socket
            .send(Message::Binary(frame(b'2', notice.as_bytes())))
            .await
            .is_err()
        {
            return Ok(());
        }
    }
    let (master, mut child) = match spawn_client(command, cols, rows) {
        Ok(value) => value,
        Err(error) => {
            audit::record_terminal(
                &hub.audit,
                &actor,
                &token_id,
                &name,
                requested_read_only,
                ("error", "internal"),
            );
            return Err(error);
        }
    };
    audit::record_terminal(
        &hub.audit,
        &actor,
        &token_id,
        &name,
        requested_read_only,
        ("ok", ""),
    );
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut token_tick = tokio::time::interval(Duration::from_secs(1));
    let watch = (!owner).then(|| hub.identity.watch(&token_id, now_unix()));
    let mut shown_exit = session.state == protectorate_session::State::Exited;
    loop {
        tokio::select! {
            message = socket.recv() => {
                let bytes = match message {
                    Some(Ok(Message::Binary(bytes))) => bytes,
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Text(_))) => continue,
                    _ => break,
                };
                if bytes.is_empty() || bytes.len() > MAX_FRAME { break; }
                match bytes[0] {
                    b'0' if !read_only && bytes.len() > 1 => {
                        let mut written = 1;
                        while written < bytes.len() {
                            let mut guard = master.writable().await?;
                            match guard.try_io(|fd| {
                                let count = unsafe { libc::write(fd.as_raw_fd(), bytes[written..].as_ptr().cast(), bytes.len() - written) };
                                if count < 0 { Err(io::Error::last_os_error()) } else { Ok(count as usize) }
                            }) {
                                Ok(Ok(0)) | Ok(Err(_)) => break,
                                Ok(Ok(count)) => written += count,
                                Err(_) => continue,
                            }
                        }
                    }
                    b'1' => {
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes[1..]) {
                            let cols = value["cols"].as_u64().and_then(|n| u16::try_from(n).ok()).filter(|n| *n > 0);
                            let rows = value["rows"].as_u64().and_then(|n| u16::try_from(n).ok()).filter(|n| *n > 0);
                            if let (Some(cols), Some(rows)) = (cols, rows) {
                                let size = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
                                unsafe { libc::ioctl(master.get_ref().as_raw_fd(), libc::TIOCSWINSZ, &size); }
                            }
                        }
                    }
                    _ => {}
                }
            }
            ready = master.readable() => {
                let mut guard = ready?;
                let mut data = [0u8; 32 << 10];
                match guard.try_io(|fd| {
                    let count = unsafe { libc::read(fd.as_raw_fd(), data.as_mut_ptr().cast(), data.len()) };
                    if count < 0 { Err(io::Error::last_os_error()) } else { Ok(count as usize) }
                }) {
                    Ok(Ok(0)) | Ok(Err(_)) => break,
                    Ok(Ok(count)) => if socket.send(Message::Binary(frame(b'0', &data[..count]))).await.is_err() { break; },
                    Err(_) => continue,
                }
            }
            _ = tick.tick(), if !shown_exit => {
                if let Ok(session) = hub.sessions.get(&name) {
                    if session.state == protectorate_session::State::Exited {
                        shown_exit = true;
                        read_only = true;
                        if socket.send(Message::Binary(frame(b'2', exit_notice(&session).as_bytes()))).await.is_err() { break; }
                    }
                }
            }
            _ = token_tick.tick(), if !owner => {
                if watch.as_ref().is_some_and(|watch| watch.is_cancelled()) { break; }
            }
        }
    }
    drop(master);
    let _ = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Err(_) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    })
    .await;
    let _ = socket.send(Message::Close(None)).await;
    Ok(())
}

fn frame(kind: u8, payload: &[u8]) -> axum::body::Bytes {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(kind);
    frame.extend_from_slice(payload);
    frame.into()
}

fn exit_notice(session: &protectorate_session::Session) -> String {
    let how = if session.exit_signal > 0 {
        format!("killed by signal {}", session.exit_signal)
    } else {
        format!("exited with code {}", session.exit_code)
    };
    format!(
        "session {} {}; showing its final screen read-only (respawn it to run again)",
        session.name, how
    )
}
