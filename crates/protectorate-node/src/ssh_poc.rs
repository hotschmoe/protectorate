//! Experimental hub-side client. The shipping fleet continues to use member.
#![forbid(unsafe_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use pt_frame::Kind;
use pt_frame::msg::{ArtifactEnd, HelloReply, JobReply, JobState, LogEnd};
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::FromJson;

const JOB: &str = "ssh-poc";
const TOKEN: &str = "ssh-poc-token";
const LIMIT: usize = 4096;

struct Destination {
    user: String,
    host: String,
    port: u16,
    identity: PathBuf,
    known_hosts: PathBuf,
    output: PathBuf,
}

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    frames: FrameReader<ChildStdout>,
    next: u64,
}

impl Session {
    fn open(dest: &Destination) -> Result<Self, String> {
        let mut child = Command::new("ssh")
            .args([
                "-F",
                "/dev/null",
                "-T",
                "-oBatchMode=yes",
                "-oIdentitiesOnly=yes",
                "-oIdentityAgent=none",
                "-oStrictHostKeyChecking=yes",
                "-oGlobalKnownHostsFile=/dev/null",
                "-oUpdateHostKeys=no",
                "-oClearAllForwardings=yes",
                "-oForwardAgent=no",
                "-oForwardX11=no",
                "-oConnectTimeout=5",
                "-oServerAliveInterval=5",
                "-oServerAliveCountMax=2",
            ])
            .arg(format!(
                "-oUserKnownHostsFile={}",
                dest.known_hosts.display()
            ))
            .arg("-i")
            .arg(&dest.identity)
            .args(["-p", &dest.port.to_string(), "-l", &dest.user, "--"])
            .arg(&dest.host)
            // A properly configured sshd ignores this and runs only its forced command.
            .arg("protectorate-ssh-poc-command-must-be-forced")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("starting ssh: {e}"))?;
        let input = child.stdin.take();
        let output = child.stdout.take().ok_or("ssh has no stdout")?;
        Ok(Self {
            child,
            input,
            frames: FrameReader::new(output, &[]),
            next: 0,
        })
    }

    fn request(&mut self, json: &str, max: usize) -> Result<(Kind, Vec<u8>, Vec<u8>), String> {
        self.next += 1;
        let id = self.next;
        let input = self.input.as_mut().ok_or("ssh input is closed")?;
        write_frame(input, Kind::Request, id, json.as_bytes())
            .and_then(|()| input.flush())
            .map_err(|e| format!("sending request {id}: {e}"))?;
        let mut bytes = Vec::new();
        loop {
            let frame = self
                .frames
                .read()
                .map_err(|e| format!("reading request {id}: {e}"))?
                .ok_or_else(|| format!("ssh closed before answering request {id}"))?;
            if frame.id != id {
                return Err(format!("request {id}: received frame for {}", frame.id));
            }
            match frame.kind {
                Kind::Chunk => {
                    if frame.payload.len() > max.saturating_sub(bytes.len()) {
                        return Err(format!("request {id}: more than {max} bytes"));
                    }
                    bytes.extend_from_slice(&frame.payload);
                }
                Kind::Response | Kind::End if bytes.is_empty() || frame.kind == Kind::End => {
                    return Ok((frame.kind, bytes, frame.payload));
                }
                Kind::Error => {
                    return Err(format!(
                        "request {id}: node refused: {}",
                        String::from_utf8_lossy(&frame.payload)
                    ));
                }
                _ => return Err(format!("request {id}: unexpected {:?}", frame.kind)),
            }
        }
    }

    fn response<T: FromJson>(&mut self, request: &str) -> Result<T, String> {
        let (kind, _, payload) = self.request(request, 0)?;
        if kind != Kind::Response {
            return Err(format!("expected response, got {kind:?}"));
        }
        pt_json::from_bytes(&payload).map_err(|e| format!("decoding response: {e}"))
    }

    fn stream<T: FromJson>(&mut self, request: &str) -> Result<(Vec<u8>, T), String> {
        let (kind, bytes, end) = self.request(request, LIMIT)?;
        if kind != Kind::End {
            return Err(format!("expected end frame, got {kind:?}"));
        }
        let end = pt_json::from_bytes(&end).map_err(|e| format!("decoding end: {e}"))?;
        Ok((bytes, end))
    }

    fn close(mut self) -> Result<(), String> {
        drop(self.input.take());
        let status = self
            .child
            .wait()
            .map_err(|e| format!("waiting for ssh: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ssh exited: {status}"))
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hello(session: &mut Session) -> Result<HelloReply, String> {
    session.response(r#"{"type":"Hello"}"#)
}

fn run(dest: &Destination) -> Result<(), String> {
    let mut first = Session::open(dest)?;
    let initial = hello(&mut first)?;
    let submitted: JobReply = first.response(
        r#"{"type":"SubmitJob","job_id":"ssh-poc","attempt":1,"token":"ssh-poc-token","script":"echo started; sleep 4; printf 'artifact over ssh\\n' > result.txt; echo finished","even_if_present":true}"#,
    )?;
    let job = submitted.job.ok_or("submit returned no job")?;
    if job.state != JobState::Running || job.token.as_deref() != Some(TOKEN) {
        return Err(format!("unexpected submit state: {:?}", job.state));
    }
    let closed = Instant::now();
    first.close()?;
    if closed.elapsed() >= Duration::from_secs(3) {
        return Err("the ssh session did not close while the job was running".into());
    }

    let mut second = Session::open(dest)?;
    let resumed = hello(&mut second)?;
    if resumed.node_id != initial.node_id || resumed.journal_epoch != initial.journal_epoch {
        return Err("node identity or journal changed across sessions".into());
    }
    let until = Instant::now() + Duration::from_secs(25);
    loop {
        let reply: JobReply =
            second.response(r#"{"type":"JobStatus","job_id":"ssh-poc","attempt":1}"#)?;
        if reply.journal_epoch != initial.journal_epoch {
            return Err("journal epoch changed while the job ran".into());
        }
        let job = reply.job.ok_or("job disappeared")?;
        if job.token.as_deref() != Some(TOKEN) {
            return Err("job token changed".into());
        }
        if job.state == JobState::Succeeded {
            break;
        }
        if job.state.is_terminal() {
            return Err(format!("job ended: {:?}", job.state));
        }
        if Instant::now() >= until {
            return Err("job did not finish within 25 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (log, log_end): (Vec<u8>, LogEnd) = second.stream(
        r#"{"type":"FetchLog","job_id":"ssh-poc","attempt":1,"token":"ssh-poc-token","max":4096}"#,
    )?;
    if !log_end.done || log_end.size != log.len() as u64 || log != b"started\nfinished\n" {
        return Err("unexpected job log".into());
    }
    let (artifact, end): (Vec<u8>, ArtifactEnd) = second.stream(
        r#"{"type":"FetchArtifact","job_id":"ssh-poc","attempt":1,"path":"result.txt","token":"ssh-poc-token","max":4096}"#,
    )?;
    let hash = pt_hash::to_hex(&pt_hash::sha256(&artifact));
    if !end.done
        || end.offset != artifact.len() as u64
        || end.size != artifact.len() as u64
        || end.sha256.as_deref() != Some(hash.as_str())
    {
        return Err("artifact size, offset or hash did not match".into());
    }
    std::fs::write(&dest.output, &artifact)
        .map_err(|e| format!("writing {}: {e}", dest.output.display()))?;
    second.close()?;
    println!(
        "node {} epoch {}: job {JOB} succeeded; {} bytes sha256 {hash}",
        initial.node_id,
        initial.journal_epoch,
        artifact.len()
    );
    Ok(())
}

fn main() {
    if let Err(e) = args().and_then(|d| run(&d)) {
        eprintln!("pt-ssh-poc: {e}");
        std::process::exit(1);
    }
}

fn args() -> Result<Destination, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("usage: pt-ssh-poc USER HOST PORT IDENTITY KNOWN_HOSTS OUTPUT".into());
    }
    if args[0].is_empty()
        || args[1].is_empty()
        || args[0].starts_with('-')
        || args[1].starts_with('-')
        || args[0].contains(['\n', '\r'])
        || args[1].contains(['\n', '\r'])
    {
        return Err("invalid user or host".into());
    }
    Ok(Destination {
        user: args[0].clone(),
        host: args[1].clone(),
        port: args[2].parse().map_err(|e| format!("port: {e}"))?,
        identity: args[3].clone().into(),
        known_hosts: args[4].clone().into(),
        output: args[5].clone().into(),
    })
}
