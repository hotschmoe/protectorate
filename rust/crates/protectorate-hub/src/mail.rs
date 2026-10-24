//! MessageService projection of the durable mailbox.
use super::{proto_time, Hub};
use chrono::Utc;
use protectorate_mailbox::{self as mailbox, Action, Kind, Message, NewMessage, ThreadInfo};
use protectorate_sleeve_registry::{Mode, State};
use protectorate_wire::{
    self as wire,
    v1::{self, message_service},
    AuthContext, Call, Code, Result, RpcError, UnaryServer,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_BODY: usize = 32 * 1024;

fn error(error: mailbox::Error) -> RpcError {
    let code = match error {
        mailbox::Error::NotFound { .. } => Code::NotFound,
        mailbox::Error::NotRecipient { .. } => Code::PermissionDenied,
        mailbox::Error::Timeout { .. } => Code::DeadlineExceeded,
        mailbox::Error::Cancelled => Code::Canceled,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn directory_error(error: protectorate_directory::Error) -> RpcError {
    let code = match error {
        protectorate_directory::Error::NotFound(_) => Code::NotFound,
        protectorate_directory::Error::Ambiguous { .. } => Code::InvalidArgument,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn who(auth: &AuthContext) -> String {
    if auth.device {
        "owner".into()
    } else {
        super::sender(&auth.actor)
    }
}

fn acting_as(auth: &AuthContext, as_name: String) -> Result<String> {
    let self_name = who(auth);
    if as_name.is_empty() || as_name == self_name {
        Ok(self_name)
    } else if auth.owner {
        Ok(as_name)
    } else {
        Err(RpcError::new(
            Code::PermissionDenied,
            "only the owner may act as another mailbox",
        ))
    }
}

fn visible(message: &Message, actor: &str, owner: bool) -> bool {
    owner || message.from == actor || message.deliveries.contains_key(actor)
}

fn mail_message(message: &Message) -> v1::MailMessage {
    let kind = match message.kind.as_str() {
        "request" => v1::MessageKind::Request,
        "notice" => v1::MessageKind::Notice,
        _ => v1::MessageKind::Note,
    };
    v1::MailMessage {
        id: message.id.clone(),
        thread_id: message.thread.clone(),
        from: message.from.clone(),
        to: message.to.clone(),
        kind: kind as i32,
        ack_required: message.ack_required,
        body: message.body.clone(),
        reply_to: message.reply_to.clone(),
        created_at: Some(proto_time(message.created)),
        deliveries: message
            .recipients
            .iter()
            .filter_map(|recipient| {
                let delivery = message.deliveries.get(recipient)?;
                Some(v1::MailDelivery {
                    recipient: recipient.clone(),
                    delivered_at: delivery.delivered.map(proto_time),
                    read_at: delivery.read.map(proto_time),
                    acked_at: delivery.acked.map(proto_time),
                    reminders: delivery.reminders.max(0).try_into().unwrap_or(u32::MAX),
                })
            })
            .collect(),
        escalated_at: message.escalated.map(proto_time),
        note_id: message.note_id.clone(),
        run_id: message.run_id.clone(),
    }
}

fn count(value: usize) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

fn thread_summary(info: &ThreadInfo) -> v1::ThreadSummary {
    v1::ThreadSummary {
        thread_id: info.id.clone(),
        participants: info.participants.clone(),
        message_count: count(info.messages),
        last: Some(mail_message(&info.last)),
        unread: count(info.unread),
        open_acks: count(info.open_acks),
        escalated: count(info.escalated),
        projects: info.projects.clone(),
    }
}

fn kind(value: i32) -> Result<Kind> {
    match v1::MessageKind::try_from(value) {
        Ok(v1::MessageKind::Unspecified | v1::MessageKind::Note) => Ok(Kind::NOTE),
        Ok(v1::MessageKind::Request) => Ok(Kind::REQUEST),
        Ok(v1::MessageKind::Notice) => Ok(Kind::NOTICE),
        Err(_) => Err(RpcError::new(Code::InvalidArgument, "unknown message kind")),
    }
}

fn request_id(id: String) -> Result<String> {
    if !id.is_empty()
        && (id.len() < 8
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
    {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "request_id must be 8 to 128 characters of letters, digits, '.', '_' and '-'",
        ));
    }
    Ok(id)
}

fn address(hub: &Hub, to: &str, from: &str, direct: bool) -> Result<(Vec<String>, String)> {
    let to = to.trim();
    if to.is_empty() {
        return Err(RpcError::new(Code::InvalidArgument, "to is required"));
    }
    if direct {
        return Ok((vec![to.into()], String::new()));
    }
    if to == "orchestrator" {
        return Ok((vec!["envoy".into()], String::new()));
    }
    if to == "owner" {
        return Ok((vec!["owner".into()], String::new()));
    }
    if let Some(project) = to.strip_prefix("project:") {
        let view = hub.directory.get(project).map_err(directory_error)?;
        let recipients = view
            .project
            .owners
            .iter()
            .filter(|owner| owner.sleeve != from)
            .filter(|owner| {
                hub.sleeves
                    .get(&owner.sleeve)
                    .is_ok_and(|row| row.state == State::Running)
            })
            .map(|owner| owner.sleeve.clone())
            .collect();
        return Ok((recipients, view.project.id));
    }
    if to == from {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "a message to yourself goes nowhere",
        ));
    }
    hub.sleeves.get(to).map_err(|_| {
        RpcError::new(
            Code::NotFound,
            format!(
                "no sleeve {to:?}; address a sleeve name, project:<name>, orchestrator or owner"
            ),
        )
    })?;
    Ok((vec![to.into()], String::new()))
}

fn delivery_mode(hub: &Hub, recipient: &str) -> &'static str {
    if recipient == "owner" {
        return "inbox";
    }
    match hub.sleeves.get(recipient) {
        Ok(row) if row.mode == Mode::Chat => "chat",
        Ok(_) => "inbox",
        Err(_) => "",
    }
}

struct Send {
    to: String,
    body: String,
    thread: String,
    kind: Kind,
    ack_required: bool,
    request_id: String,
    reply_to: String,
    direct: bool,
    footer: Option<mailbox::Footer>,
}

fn send(hub: &Hub, from: &str, args: Send) -> Result<(Message, bool)> {
    if args.body.trim().is_empty() {
        return Err(RpcError::new(Code::InvalidArgument, "body is required"));
    }
    if args.body.len() > MAX_BODY {
        return Err(RpcError::new(
            Code::InvalidArgument,
            format!("body is {} bytes; the limit is {MAX_BODY}", args.body.len()),
        ));
    }
    if !args.thread.is_empty() {
        let messages = hub.mail.thread(&args.thread);
        if messages.is_empty() {
            return Err(RpcError::new(
                Code::NotFound,
                format!("no thread {:?}", args.thread),
            ));
        }
        if from != "owner"
            && from != "hub"
            && !messages.iter().any(|message| visible(message, from, false))
        {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "not a participant in this thread",
            ));
        }
    }
    let (recipients, project) = address(hub, &args.to, from, args.direct)?;
    let from_project = hub
        .directory
        .projects_of(from)
        .into_iter()
        .next()
        .unwrap_or_default();
    let outcome = hub
        .mail
        .send(NewMessage {
            from: from.into(),
            to: args.to.trim().into(),
            kind: args.kind,
            ack_required: args.ack_required,
            body: args.body,
            request_id: args.request_id,
            thread: args.thread,
            reply_to: args.reply_to,
            from_project,
            project: project.clone(),
            recipients,
            footer: args.footer,
            run_id: super::run_of(&hub.sleeves, from),
        })
        .map_err(error)?;
    let mut message = outcome.message;
    if outcome.created {
        hub.emit(
            "msg.sent",
            &message.id,
            json!({"recipients": message.recipients.join(",")}),
        );
        for recipient in &message.recipients {
            if delivery_mode(hub, recipient) == "inbox" {
                hub.mail.arm(&message.id, recipient).map_err(error)?;
            }
        }
        if message.kind == Kind::REQUEST && message.recipients.is_empty() && !project.is_empty() {
            let display = hub
                .directory
                .get(&project)
                .map_err(directory_error)?
                .project
                .display;
            let title = message.body.lines().next().unwrap_or("").trim();
            let note = hub
                .backlog
                .create(super::NewNote {
                    title: format!("{} asks: {}", message.from, title.chars().take(80).collect::<String>()),
                    body: format!(
                        "{}\n\nFrom {}, thread {}, message {}.\nReply: protectorate msg reply {} \"...\"",
                        message.body.trim_end_matches('\n'), message.from, message.thread,
                        message.id, message.id,
                    ),
                    project: display,
                    source: message.from.clone(),
                    pinned: false,
                })
                .map_err(super::backlog_error)?;
            hub.mail.set_note_id(&message.id, &note.id).map_err(error)?;
            hub.emit("note.created", &note.id, super::note_event_payload(&note));
            message.note_id = note.id;
        }
    }
    Ok((message, !outcome.created))
}

pub(super) fn send_release_notice(
    hub: &Hub,
    to: &str,
    body: String,
    request_id: String,
    reply_via: String,
) -> Result<()> {
    send(
        hub,
        "hub",
        Send {
            to: to.into(),
            body,
            thread: String::new(),
            kind: Kind::NOTICE,
            ack_required: false,
            request_id,
            reply_to: String::new(),
            direct: false,
            footer: Some(Box::new(move |thread| {
                format!("Reply: protectorate msg send {reply_via} --thread {thread} \"...\"")
            })),
        },
    )?;
    Ok(())
}

fn ack(hub: &Hub, id: &str, actor: &str) -> Result<Message> {
    let (message, changed) = hub.mail.ack(id, actor).map_err(error)?;
    if changed {
        hub.emit("msg.acked", &message.id, json!({"recipient": actor}));
    }
    Ok(message)
}

fn delivery_pass(hub: &Hub, scanned: &mut BTreeMap<String, u64>, envoy_retry: &mut Instant) {
    for message in hub.mail.unresolved() {
        if message.project.is_empty() {
            continue;
        }
        let Ok(view) = hub.directory.get(&message.project) else {
            continue;
        };
        let recipients: Vec<_> = view
            .project
            .owners
            .iter()
            .filter(|owner| owner.sleeve != message.from)
            .filter(|owner| {
                hub.sleeves
                    .get(&owner.sleeve)
                    .is_ok_and(|row| row.state == State::Running)
            })
            .map(|owner| owner.sleeve.clone())
            .collect();
        if recipients.is_empty() {
            continue;
        }
        let armed: Vec<_> = recipients
            .iter()
            .filter(|name| delivery_mode(hub, name) == "inbox")
            .cloned()
            .collect();
        if let Err(error) = hub.mail.resolve(&message.id, &recipients, &armed) {
            eprintln!("mailbox: resolve {}: {error}", message.id);
        }
    }
    for (recipient, messages) in hub.mail.pending() {
        reconcile_chat_log(hub, &recipient, &messages, scanned);
        let messages = hub.mail.pending_for(&recipient);
        if recipient == "envoy" && !messages.is_empty() && Instant::now() >= *envoy_retry {
            match crate::chat::resume_envoy_for_mail(hub) {
                Ok(true) => *envoy_retry = Instant::now() + Duration::from_secs(60),
                Ok(false) => {}
                Err(error) => {
                    eprintln!("mailbox: resume envoy: {error}");
                    *envoy_retry = Instant::now() + Duration::from_secs(60);
                }
            }
        }
        match delivery_mode(hub, &recipient) {
            "inbox" => {
                for message in messages {
                    if let Err(error) = hub.mail.arm(&message.id, &recipient) {
                        eprintln!("mailbox: arm {} for {recipient}: {error}", message.id);
                    }
                }
            }
            "chat" if !messages.is_empty() => {
                if !Utc::now()
                    .signed_duration_since(messages[0].created)
                    .to_std()
                    .is_ok_and(|elapsed| elapsed >= Duration::from_secs(2))
                {
                    continue;
                }
                inject(hub, &recipient, &messages[..messages.len().min(32)], false);
            }
            _ => {}
        }
    }
    process_overdue(hub);
}

fn reconcile_chat_log(
    hub: &Hub,
    recipient: &str,
    pending: &[Message],
    scanned: &mut BTreeMap<String, u64>,
) {
    if delivery_mode(hub, recipient) != "chat" || pending.is_empty() {
        return;
    }
    let path = hub
        .workspaces
        .state_root()
        .join("sleeves")
        .join(recipient)
        .join("chat.jsonl");
    let Ok(size) = fs::metadata(&path).map(|meta| meta.len()) else {
        return;
    };
    if scanned.get(recipient) == Some(&size) {
        return;
    }
    let Ok(file) = fs::File::open(path) else {
        return;
    };
    let mut complete = true;
    // Chat logs can be large. Scan one event at a time instead of collecting
    // the entire transcript just to recover mailbox request IDs.
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { return };
        let Ok(event) = serde_json::from_str::<protectorate_chatlog::Event>(&line) else {
            return;
        };
        let user = &event.payload["user"];
        let Some(request_id) = user["requestId"]
            .as_str()
            .and_then(|s| s.strip_prefix("mailbox-"))
        else {
            continue;
        };
        if !user["from"].as_str().is_some_and(|from| {
            from == "owner" || from == "hub" || from == "mailbox" || from.starts_with("sleeve:")
        }) {
            continue;
        }
        if !user["text"]
            .as_str()
            .is_some_and(|s| s.contains(mailbox::CONCISE_GUIDANCE))
        {
            continue;
        }
        for id in request_id.split('-').take_while(|id| {
            id.starts_with('m') && id.len() > 1 && id[1..].bytes().all(|b| b.is_ascii_digit())
        }) {
            if let Some(message) = pending.iter().find(|message| message.id == id) {
                if let Err(error) = hub.mail.deliver(id, recipient) {
                    eprintln!("mailbox: reconcile delivery {id} to {recipient}: {error}");
                    complete = false;
                } else {
                    hub.emit(
                        "msg.delivered",
                        id,
                        json!({"recipient":recipient,
                        "from":message.from, "to":message.to, "thread":message.thread,
                        "kind":message.kind.as_str()}),
                    );
                }
            }
        }
    }
    if complete {
        scanned.insert(recipient.into(), size);
    }
}

fn process_overdue(hub: &Hub) {
    for due in hub.mail.overdue(hub.msg_ack_timeout) {
        let Some(message) = hub.mail.get(&due.id) else {
            continue;
        };
        let Some(delivery) = message.deliveries.get(&due.rcpt) else {
            continue;
        };
        match due.action {
            Action::Remind => {
                if delivery.read.is_some() || delivery_mode(hub, &due.rcpt) == "inbox" {
                    if let Err(error) = hub.mail.remind(&due.id, &due.rcpt) {
                        eprintln!("mailbox: remind {}: {error}", due.id);
                    } else {
                        hub.emit(
                            "msg.reminded",
                            &due.id,
                            json!({"recipient":due.rcpt, "read":delivery.read.is_some()}),
                        );
                    }
                } else if delivery_mode(hub, &due.rcpt) == "chat" {
                    inject(hub, &due.rcpt, &[message], true);
                }
            }
            Action::Escalate => {
                if let Err(error) = hub.mail.escalate(&due.id) {
                    eprintln!("mailbox: escalate {}: {error}", due.id);
                    continue;
                }
                hub.emit("msg.escalated", &due.id, json!({"recipient":due.rcpt}));
                if due.rcpt != "envoy"
                    && hub
                        .sleeves
                        .get("envoy")
                        .is_ok_and(|row| row.state == State::Running)
                {
                    let first_line = message
                        .body
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(300)
                        .collect::<String>();
                    let body = format!("Message {} (thread {}) from {} to {} asked for an ack and {} has not given one after a reminder.\n\nIt said: {}", message.id, message.thread, message.from, message.to, due.rcpt, first_line);
                    if let Err(error) = send(
                        hub,
                        "hub",
                        Send {
                            to: "envoy".into(),
                            body,
                            thread: message.thread.clone(),
                            kind: Kind::NOTICE,
                            ack_required: false,
                            request_id: format!("escalate-{}", message.id),
                            reply_to: String::new(),
                            direct: true,
                            footer: None,
                        },
                    ) {
                        eprintln!("mailbox: notify envoy of {}: {error:?}", message.id);
                    }
                }
            }
        }
    }
}

fn inject(hub: &Hub, recipient: &str, messages: &[Message], reminder: bool) -> bool {
    if messages.is_empty() || !super::chat::chat_idle(hub, recipient) {
        return false;
    }
    let parts = messages
        .iter()
        .map(|message| {
            let project = if message.project.is_empty() {
                String::new()
            } else {
                hub.directory
                    .get(&message.project)
                    .map(|view| view.project.display)
                    .unwrap_or_else(|_| message.project.clone())
            };
            render_message(message, &project, reminder)
        })
        .collect::<Vec<_>>();
    let request_id = batch_id(messages, recipient, reminder);
    let from = if messages.len() == 1 {
        match messages[0].from.as_str() {
            "owner" | "hub" => messages[0].from.clone(),
            other => format!("sleeve:{other}"),
        }
    } else {
        "mailbox".into()
    };
    let body = format!(
        "{}\n\n{}\n\n{}",
        parts.join("\n\n---\n\n"),
        mailbox::CONCISE_GUIDANCE,
        mailbox::SUBAGENT_GUIDANCE
    );
    if super::chat::send_mailbox(hub, recipient, &body, &request_id, &from).is_err() {
        return false;
    }
    for message in messages {
        let result = if reminder {
            hub.mail.remind(&message.id, recipient)
        } else {
            hub.mail.deliver(&message.id, recipient)
        };
        match result {
            Ok(()) => hub.emit(if reminder { "msg.reminded" } else { "msg.delivered" }, &message.id,
                json!({"recipient":recipient, "from":message.from, "to":message.to, "thread":message.thread, "kind":message.kind.as_str()})),
            Err(error) => eprintln!("mailbox: record delivery {} to {recipient}: {error}", message.id),
        }
    }
    true
}

fn batch_id(messages: &[Message], recipient: &str, reminder: bool) -> String {
    // The ids also let a restarted hub reconcile a persisted chat prompt
    // sent just before the mailbox delivery record was committed.
    let ids = messages
        .iter()
        .map(|m| m.id.as_str())
        .collect::<Vec<_>>()
        .join("-");
    if reminder {
        let count = messages[0].deliveries[recipient].reminders + 1;
        format!("mailbox-{ids}-r{count}")
    } else {
        format!("mailbox-{ids}")
    }
}

fn render_message(message: &Message, project: &str, reminder: bool) -> String {
    let kind = if message.kind == Kind::NOTE {
        "message"
    } else {
        message.kind.as_str()
    };
    let from = if message.from == "owner" || message.from == "hub" {
        message.from.clone()
    } else {
        format!("sleeve {}", message.from)
    };
    let mut header = format!(
        "{kind} {} from {from}, thread {}",
        message.id, message.thread
    );
    if !project.is_empty() {
        header.push_str(&format!(", to project {project}"));
    }
    if message.ack_required {
        header.push_str(", ack required");
    }
    if reminder {
        header = format!("reminder, still unacked: {header}");
    }
    let mut body = format!("[{header}]\n{}", message.body.trim_end_matches('\n'));
    if !message.note_id.is_empty() {
        body.push_str(&format!("\n\nThis request is already filed as note {} for the project. Work it from the note (protectorate backlog show {}); do not add another note for it.", message.note_id, message.note_id));
    }
    if message.from != "hub" {
        body.push_str(&format!(
            "\nReply: protectorate msg reply {} \"...\"",
            message.id
        ));
    }
    if message.ack_required || message.from != "hub" {
        body.push_str(&format!("\nAck: protectorate msg ack {}", message.id));
    }
    body
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    // The weak reference ends coordination when the router and its handlers
    // are dropped, including in tests and during a clean server shutdown.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        let worker = Arc::downgrade(&hub);
        runtime.spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(500));
            let mut scanned = BTreeMap::<String, u64>::new();
            let mut envoy_retry = Instant::now();
            loop {
                ticker.tick().await;
                let Some(hub) = worker.upgrade() else {
                    break;
                };
                match tokio::task::spawn_blocking(move || {
                    delivery_pass(&hub, &mut scanned, &mut envoy_retry);
                    (scanned, envoy_retry)
                })
                .await
                {
                    Ok((next_scanned, next_retry)) => {
                        scanned = next_scanned;
                        envoy_retry = next_retry;
                    }
                    Err(error) => {
                        eprintln!("mailbox coordination failed: {error}");
                        break;
                    }
                }
            }
        });
    }
    let send_hub = Arc::clone(&hub);
    server.unary(
        message_service::SEND,
        move |call: Call<v1::MessageServiceSendRequest>| {
            let hub = Arc::clone(&send_hub);
            async move {
                let request = call.request;
                let kind = kind(request.kind)?;
                let ack_required = request.ack_required.unwrap_or(kind == Kind::REQUEST);
                let (message, duplicate) = send(
                    &hub,
                    &who(&call.auth),
                    Send {
                        to: request.to,
                        body: request.body,
                        thread: request.thread_id,
                        kind,
                        ack_required,
                        request_id: request_id(request.request_id)?,
                        reply_to: String::new(),
                        direct: false,
                        footer: None,
                    },
                )?;
                Ok(v1::MessageServiceSendResponse {
                    message: Some(mail_message(&message)),
                    duplicate,
                    waiting: message.recipients.is_empty(),
                    note_id: message.note_id,
                })
            }
        },
    )?;
    let inbox_hub = Arc::clone(&hub);
    server.unary(
        message_service::INBOX,
        move |call: Call<v1::InboxRequest>| {
            let hub = Arc::clone(&inbox_hub);
            async move {
                let request = call.request;
                let actor = acting_as(&call.auth, request.r#as)?;
                let messages = hub
                    .mail
                    .inbox(
                        &actor,
                        &request.thread_id,
                        request.unread_only,
                        request.peek,
                    )
                    .map_err(error)?;
                if !request.peek {
                    for message in &messages {
                        if message
                            .deliveries
                            .get(&actor)
                            .is_some_and(|d| d.read.is_none())
                        {
                            hub.emit("msg.read", &message.id, json!({"recipient": actor}));
                        }
                    }
                }
                Ok(v1::InboxResponse {
                    messages: messages.iter().map(mail_message).collect(),
                })
            }
        },
    )?;
    let ack_hub = Arc::clone(&hub);
    server.unary(message_service::ACK, move |call: Call<v1::AckRequest>| {
        let hub = Arc::clone(&ack_hub);
        async move {
            let actor = acting_as(&call.auth, call.request.r#as)?;
            let message = ack(&hub, &call.request.id, &actor)?;
            Ok(v1::AckResponse {
                message: Some(mail_message(&message)),
            })
        }
    })?;
    let reply_hub = Arc::clone(&hub);
    server.unary(
        message_service::REPLY,
        move |call: Call<v1::ReplyRequest>| {
            let hub = Arc::clone(&reply_hub);
            async move {
                let request = call.request;
                let actor = who(&call.auth);
                let id = request.id;
                let original = hub
                    .mail
                    .get(&id)
                    .ok_or_else(|| RpcError::new(Code::NotFound, format!("no message {id:?}")))?;
                if !call.auth.owner && !original.deliveries.contains_key(&actor) {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        format!("{actor} is not a recipient of {id}"),
                    ));
                }
                if original.from == "hub" {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        format!("{id} is a notice from the hub; ack it instead"),
                    ));
                }
                let (message, duplicate) = send(
                    &hub,
                    &actor,
                    Send {
                        to: original.from.clone(),
                        body: request.body,
                        thread: original.thread,
                        kind: Kind::NOTE,
                        ack_required: false,
                        request_id: request_id(request.request_id)?,
                        reply_to: id.clone(),
                        direct: true,
                        footer: None,
                    },
                )?;
                if !duplicate && original.deliveries.contains_key(&actor) {
                    ack(&hub, &id, &actor)?;
                }
                Ok(v1::ReplyResponse {
                    message: Some(mail_message(&message)),
                })
            }
        },
    )?;
    let thread_hub = Arc::clone(&hub);
    server.unary(
        message_service::THREAD,
        move |call: Call<v1::ThreadRequest>| {
            let hub = Arc::clone(&thread_hub);
            async move {
                let id = call.request.thread_id;
                let messages = hub.mail.thread(&id);
                if messages.is_empty() {
                    return Err(RpcError::new(Code::NotFound, format!("no thread {id:?}")));
                }
                let actor = who(&call.auth);
                if !messages
                    .iter()
                    .any(|message| visible(message, &actor, call.auth.owner))
                {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "not a participant in this thread",
                    ));
                }
                Ok(v1::ThreadResponse {
                    messages: messages.iter().map(mail_message).collect(),
                })
            }
        },
    )?;
    let list_hub = Arc::clone(&hub);
    server.unary(
        message_service::LIST_THREADS,
        move |call: Call<v1::ListThreadsRequest>| {
            let hub = Arc::clone(&list_hub);
            async move {
                let request = call.request;
                if request.limit < 0 {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "limit must not be negative",
                    ));
                }
                let limit = if request.limit == 0 {
                    100
                } else {
                    request.limit.min(500) as usize
                };
                let sleeve = request.sleeve.trim();
                let query = request.project.trim();
                let project = if query.is_empty() {
                    String::new()
                } else {
                    hub.directory
                        .get(query.trim_start_matches("project:"))
                        .map_err(directory_error)?
                        .project
                        .id
                };
                let actor = who(&call.auth);
                Ok(v1::ListThreadsResponse {
                    threads: hub
                        .mail
                        .threads(&actor)
                        .into_iter()
                        .filter(|thread| call.auth.owner || thread.participants.contains(&actor))
                        .filter(|thread| {
                            sleeve.is_empty() || thread.participants.iter().any(|p| p == sleeve)
                        })
                        .filter(|thread| project.is_empty() || thread.projects.contains(&project))
                        .take(limit)
                        .map(|thread| thread_summary(&thread))
                        .collect(),
                })
            }
        },
    )?;
    server.unary(
        message_service::WAIT,
        move |call: Call<v1::MessageServiceWaitRequest>| {
            let hub = Arc::clone(&hub);
            async move {
                let target = call.request.target;
                let duration = match &call.request.timeout {
                    None => Duration::from_secs(600),
                    Some(timeout) => wire::duration(Some(timeout), "timeout")?,
                };
                if duration.is_zero() || duration > Duration::from_secs(86_400) {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "timeout must be positive and at most 24h",
                    ));
                }
                let thread = if let Some(message) = hub.mail.get(&target) {
                    message.thread
                } else {
                    target.clone()
                };
                let messages = hub.mail.thread(&thread);
                if messages.is_empty() {
                    return Err(RpcError::new(
                        Code::NotFound,
                        format!("no message or thread {target:?}"),
                    ));
                }
                let actor = who(&call.auth);
                if !messages
                    .iter()
                    .any(|message| visible(message, &actor, call.auth.owner))
                {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "not a participant in this thread",
                    ));
                }
                match hub.mail.wait_async(&target, &actor, Some(duration)).await {
                    Ok(message) => Ok(v1::MessageServiceWaitResponse {
                        message: Some(mail_message(&message)),
                        timed_out: false,
                    }),
                    Err(mailbox::Error::Timeout { .. }) => Ok(v1::MessageServiceWaitResponse {
                        timed_out: true,
                        ..Default::default()
                    }),
                    Err(failure) => Err(error(failure)),
                }
            }
        },
    )?;
    Ok(())
}
