//! Owner CLI desktop actions. Routing and flag parsing live in `args`/`lib`.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_connect_client::Client;
use protectorate_wire::{Code, Result, RpcError};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

const DESK: &str = "/protectorate.v1.DeskService/";
const NODE: &str = "/protectorate.v1.NodeService/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Screenshot {
        output: PathBuf,
        max_width: i32,
    },
    Windows,
    Foreground,
    Click {
        x: i32,
        y: i32,
        button: String,
        double: bool,
    },
    Move {
        x: i32,
        y: i32,
    },
    Type {
        text: String,
    },
    Key {
        chord: String,
    },
    Run {
        program: String,
        args: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyAction {
    Show,
    Clear,
    Set {
        window: String,
        idle: Duration,
        timezone: String,
        lock_after: bool,
    },
}

fn invalid(message: impl Into<String>) -> RpcError {
    RpcError::new(Code::InvalidArgument, message)
}

fn field<'a>(value: &'a Value, name: &str) -> &'a str {
    value.get(name).and_then(Value::as_str).unwrap_or("")
}

fn number(value: &Value, name: &str) -> i64 {
    value.get(name).and_then(Value::as_i64).unwrap_or(0)
}

fn line(value: &Value) -> Result<String> {
    serde_json::to_string(value)
        .map(|text| format!("{text}\n"))
        .map_err(|error| RpcError::new(Code::Internal, error.to_string()))
}

fn action_request(node: &str, lease: &str, action: &Action) -> Result<(&'static str, Value)> {
    if node.trim().is_empty() {
        return Err(invalid("desk needs a node name"));
    }
    let target = json!({"name":node,"leaseId":lease});
    let request = match action {
        Action::Screenshot { max_width, .. } => {
            if *max_width < 0 {
                return Err(invalid("--max-width must be nonnegative"));
            }
            ("Screenshot", json!({"target":target,"maxWidth":max_width}))
        }
        Action::Windows => ("ListWindows", json!({"target":target})),
        Action::Foreground => ("Foreground", json!({"target":target})),
        Action::Click {
            x,
            y,
            button,
            double,
        } => {
            if !matches!(button.as_str(), "" | "left" | "right" | "middle") {
                return Err(invalid("--button must be left, right or middle"));
            }
            (
                "Click",
                json!({"target":target,"x":x,"y":y,"button":button,"double":double}),
            )
        }
        Action::Move { x, y } => ("Move", json!({"target":target,"x":x,"y":y})),
        Action::Type { text } => ("Type", json!({"target":target,"text":text})),
        Action::Key { chord } => {
            if chord.trim().is_empty() {
                return Err(invalid("key needs a chord"));
            }
            ("Key", json!({"target":target,"chord":chord}))
        }
        Action::Run { program, args } => {
            if program.is_empty() {
                return Err(invalid("run needs a program"));
            }
            (
                "Run",
                json!({"target":target,"program":program,"args":args}),
            )
        }
    };
    Ok(request)
}

/// Run one desktop command. The hub acquires or renews a short lease for
/// mutating actions when `lease` is empty.
pub(crate) fn run_action(
    client: &Client,
    node: &str,
    lease: &str,
    action: &Action,
    json_output: bool,
) -> Result<String> {
    run_action_with(node, lease, action, json_output, |procedure, request| {
        client
            .call(procedure, request)
            .map(|response| (response, None))
    })
    .map(|(text, _notice)| text)
}

/// Use this entry point when the transport can return the hub's automatic
/// desk lease headers. `notice` is returned separately for stderr.
pub(crate) fn run_action_with(
    node: &str,
    lease: &str,
    action: &Action,
    json_output: bool,
    call: impl FnOnce(&str, &Value) -> Result<(Value, Option<String>)>,
) -> Result<(String, Option<String>)> {
    let (method, request) = action_request(node, lease, action)?;
    let (mut response, notice) = call(&format!("{DESK}{method}"), &request)?;
    let text = render_action(&mut response, action, json_output)?;
    Ok((text, notice))
}

fn render_action(response: &mut Value, action: &Action, json_output: bool) -> Result<String> {
    if let Action::Screenshot { output, .. } = action {
        let png = STANDARD.decode(field(response, "png")).map_err(|error| {
            RpcError::new(Code::DataLoss, format!("screenshot PNG encoding: {error}"))
        })?;
        if png.is_empty() {
            return Err(RpcError::new(Code::DataLoss, "empty screenshot"));
        }
        protectorate_fsutil::write_file(output, &png, 0o644).map_err(|error| {
            RpcError::new(
                Code::Internal,
                format!("write screenshot {}: {error}", output.display()),
            )
        })?;
        if let Some(map) = response.as_object_mut() {
            map.remove("png");
        }
        if json_output {
            return line(response);
        }
        return Ok(format!(
            "screenshot\t{}\nsize\t{}x{}\n",
            output.display(),
            number(response, "width"),
            number(response, "height")
        ));
    }
    if json_output {
        return line(response);
    }
    match action {
        Action::Windows => {
            let mut out = String::from("PID\tX\tY\tWIDTH\tHEIGHT\tTITLE\n");
            for window in response
                .get("windows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let minimized = if window.get("minimized") == Some(&Value::Bool(true)) {
                    " (minimized)"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}{}\n",
                    number(window, "pid"),
                    number(window, "x"),
                    number(window, "y"),
                    number(window, "width"),
                    number(window, "height"),
                    field(window, "title"),
                    minimized
                ));
            }
            Ok(out)
        }
        Action::Foreground => {
            if number(response, "pid") == 0 {
                return Ok("foreground\tnone\n".into());
            }
            Ok(format!(
                "title\t{}\npid\t{}\nprocess\t{}\nintegrity\t{}\nagent\t{}\nblocks\t{}\n",
                field(response, "title"),
                number(response, "pid"),
                field(response, "process"),
                field(response, "integrity"),
                field(response, "agentIntegrity"),
                response
                    .get("blocks")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ))
        }
        Action::Click { .. } => Ok("click\tok\n".into()),
        Action::Move { .. } => Ok("move\tok\n".into()),
        Action::Type { .. } => Ok("type\tok\n".into()),
        Action::Key { .. } => Ok("key\tok\n".into()),
        Action::Run { .. } => Ok(format!("started\tpid {}\n", number(response, "pid"))),
        Action::Screenshot { .. } => unreachable!(),
    }
}

fn duration_proto(duration: Duration) -> Result<String> {
    if duration.is_zero() || duration.as_secs() > i64::MAX as u64 {
        return Err(invalid("--idle must be a positive duration"));
    }
    if duration.subsec_nanos() == 0 {
        Ok(format!("{}s", duration.as_secs()))
    } else {
        let nanos = format!("{:09}", duration.subsec_nanos());
        Ok(format!(
            "{}.{}s",
            duration.as_secs(),
            nanos.trim_end_matches('0')
        ))
    }
}

fn policy_request(node: &str, action: &PolicyAction) -> Result<(&'static str, Value)> {
    if node.trim().is_empty() {
        return Err(invalid("desk policy needs a node name"));
    }
    Ok(match action {
        PolicyAction::Show => ("GetDeskPolicy", json!({"name":node})),
        PolicyAction::Clear => ("SetDeskPolicy", json!({"name":node})),
        PolicyAction::Set {
            window,
            idle,
            timezone,
            lock_after,
        } => {
            if window.is_empty() {
                return Err(invalid("--window is required"));
            }
            (
                "SetDeskPolicy",
                json!({"name":node,"policy":{"window":window,"idle":duration_proto(*idle)?,"timezone":timezone,"lockAfter":lock_after}}),
            )
        }
    })
}

pub(crate) fn run_policy(
    client: &Client,
    node: &str,
    action: &PolicyAction,
    json_output: bool,
) -> Result<String> {
    let (method, request) = policy_request(node, action)?;
    let response = client.call(&format!("{NODE}{method}"), &request)?;
    if json_output {
        return line(&response);
    }
    Ok(policy_text(node, &response))
}

fn policy_text(node: &str, response: &Value) -> String {
    let mut out = format!("node\t{node}\n");
    if let Some(policy) = response.get("policy").filter(|p| !p.is_null()) {
        let timezone = match field(policy, "timezone") {
            "" => "the node's local time",
            value => value,
        };
        out.push_str(&format!(
            "policy\twindow {} in {}, idle {}, lock after {}\n",
            field(policy, "window"),
            timezone,
            field(policy, "idle"),
            policy
                .get("lockAfter")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ));
    } else {
        out.push_str("policy\tnone: the desktop is always available\n");
    }
    let holder = field(response, "holder").trim_start_matches("auto:");
    let state = response.get("state");
    let status = if !holder.is_empty() {
        format!("in use by {holder}")
    } else {
        match state.map(|s| field(s, "reason")) {
            None => "-",
            Some("DESK_REASON_AVAILABLE")
                if response.get("policy").is_some_and(|p| !p.is_null()) =>
            {
                "away"
            }
            Some("DESK_REASON_AVAILABLE") => "available",
            Some("DESK_REASON_OUTSIDE_WINDOW") => "outside window",
            Some("DESK_REASON_USER_ACTIVE") => "user active",
            Some("DESK_REASON_NOT_INTERACTIVE") => "unavailable: not an interactive session",
            Some("DESK_REASON_LOCKED") => "unavailable: screen locked",
            Some("DESK_REASON_UNSUPPORTED") => "-",
            _ => "unknown",
        }
        .to_owned()
    };
    out.push_str(&format!("status\t{status}\n"));
    if let Some(state) = state {
        let detail = field(state, "detail");
        if !detail.is_empty() {
            out.push_str(&format!("detail\t{detail}\n"));
        }
    }
    if let Some(inputs) = response
        .get("inputs")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
    {
        let labels = inputs
            .iter()
            .map(|item| format!("{} by {}", number(item, "count"), field(item, "caller")))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("desk inputs today\t{labels}\n"));
    }
    out
}
