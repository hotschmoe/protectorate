//! The hub end of the node link: pt-frame requests over a stream the hub member dialed to
//! `<node>:7475`. One request at a time; every call has a deadline, and a connection that timed
//! out or broke protocol is dead (dial again).

use std::io::{self, BufWriter, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use pt_frame::Kind;
use pt_frame::msg::{
    self, ArtifactEnd, ErrorReply, FetchArtifact, FetchLog, Hello, HelloReply, Inventory, JobList,
    JobRef, JobReply, JobStatus, LogEnd, Presence, Request, Sample, SubmitJob, Update, UpdateReply,
};
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::FromJson;

use crate::error::{Error, Result};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Bytes accepted per FetchLog or FetchArtifact beyond what was asked for, as slack for chunk
/// framing.
const FETCH_SLACK: u64 = 64 << 10;

pub struct NodeClient {
    target: String,
    reader: FrameReader<DeadlineStream>,
    writer: BufWriter<DeadlineStream>,
    timeout: Duration,
    protocol: Option<u32>,
    next_id: u64,
    broken: Option<String>,
    disconnected: bool,
}

pub(crate) struct DeadlineStream {
    pub(crate) stream: UnixStream,
    pub(crate) deadline: Instant,
}

impl DeadlineStream {
    fn remaining(&self) -> io::Result<Duration> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            Err(io::Error::new(io::ErrorKind::TimedOut, "deadline expired"))
        } else {
            Ok(left)
        }
    }
}

impl Read for DeadlineStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(buf)
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

enum Answer {
    Response(Vec<u8>),
    /// Chunk bytes and the end frame's payload.
    Stream(Vec<u8>, Vec<u8>),
}

impl NodeClient {
    pub(crate) fn new(target: &str, stream: UnixStream) -> Result<NodeClient> {
        let dup = |s: &UnixStream| {
            s.try_clone()
                .map_err(|e| Error::new(format!("{target}: {e}")))
        };
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        Ok(NodeClient {
            target: target.to_string(),
            reader: FrameReader::new(
                DeadlineStream {
                    stream: dup(&stream)?,
                    deadline,
                },
                &[],
            ),
            writer: BufWriter::new(DeadlineStream { stream, deadline }),
            timeout: DEFAULT_TIMEOUT,
            protocol: None,
            next_id: 1,
            broken: None,
            disconnected: false,
        })
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    /// The deadline for a whole call, including every read and write.
    pub fn set_timeout(&mut self, t: Duration) -> Result<()> {
        if t.is_zero() || Instant::now().checked_add(t).is_none() {
            return Err(Error::new(format!("{}: invalid call timeout", self.target)));
        }
        self.timeout = t;
        Ok(())
    }

    pub fn hello(&mut self) -> Result<HelloReply> {
        let hello: HelloReply = self.typed(&Request::Hello(Hello {
            hub_version: Some(pt_version::VERSION.into()),
        }))?;
        self.protocol = Some(hello.protocol);
        Ok(hello)
    }

    pub fn require_compatible(&mut self) -> Result<()> {
        let protocol = match self.protocol {
            Some(p) => p,
            None => self.hello()?.protocol,
        };
        if protocol != msg::PROTOCOL {
            return Err(Error::Protocol {
                node: protocol,
                hub: msg::PROTOCOL,
            });
        }
        Ok(())
    }

    pub fn disconnected(&self) -> bool {
        self.disconnected
    }

    pub fn inventory(&mut self) -> Result<Inventory> {
        self.typed(&Request::Inventory)
    }

    pub fn sample(&mut self) -> Result<Sample> {
        self.typed(&Request::Sample)
    }

    pub fn presence(&mut self) -> Result<Presence> {
        self.typed(&Request::Presence)
    }

    pub fn submit_job(&mut self, job: SubmitJob) -> Result<JobReply> {
        self.typed(&Request::SubmitJob(job))
    }

    pub fn job_status(&mut self, job_id: &str, attempt: Option<u64>) -> Result<JobReply> {
        self.typed(&Request::JobStatus(JobStatus {
            job_id: job_id.into(),
            attempt,
        }))
    }

    /// Up to `q.max` log bytes from `q.offset`, and the end frame saying where the next fetch
    /// starts.
    pub fn fetch_log(&mut self, q: FetchLog) -> Result<(Vec<u8>, LogEnd)> {
        let max = q.max;
        self.stream(&Request::FetchLog(q), max)
    }

    /// Up to `q.max` bytes of an artifact from `q.offset`, and the end frame with its size (and
    /// its SHA-256 once the end is reached).
    pub fn fetch_artifact(&mut self, q: FetchArtifact) -> Result<(Vec<u8>, ArtifactEnd)> {
        let max = q.max;
        self.stream(&Request::FetchArtifact(q), max)
    }

    pub fn cancel_job(&mut self, job: JobRef) -> Result<JobReply> {
        self.typed(&Request::CancelJob(job))
    }

    pub fn ack_collected(&mut self, job: JobRef) -> Result<JobReply> {
        self.typed(&Request::AckCollected(job))
    }

    pub fn list_jobs(&mut self) -> Result<JobList> {
        self.typed(&Request::ListJobs)
    }

    /// The node downloads and checks the release before it answers: give it a long timeout.
    pub fn update(&mut self, u: Update) -> Result<UpdateReply> {
        self.typed(&Request::Update(u))
    }

    fn typed<T: FromJson>(&mut self, req: &Request) -> Result<T> {
        let method = req.method();
        match self.call(req, 0)? {
            Answer::Response(payload) => pt_json::from_bytes(&payload)
                .map_err(|e| self.poison(&format!("{method} answer: {e}"))),
            Answer::Stream(..) => Err(self.poison(&format!("chunk frames for {method}"))),
        }
    }

    fn stream<T: FromJson>(&mut self, req: &Request, max: u64) -> Result<(Vec<u8>, T)> {
        let method = req.method();
        match self.call(req, max.saturating_add(FETCH_SLACK))? {
            Answer::Stream(bytes, end) => {
                let end = pt_json::from_bytes(&end)
                    .map_err(|e| self.poison(&format!("{method} end frame: {e}")))?;
                Ok((bytes, end))
            }
            Answer::Response(_) => Err(self.poison(&format!("a response frame to {method}"))),
        }
    }

    fn poison(&mut self, what: &str) -> Error {
        let e = format!("{}: protocol: {what}", self.target);
        self.broken = Some(e.clone());
        Error::new(e)
    }

    fn call(&mut self, req: &Request, log_max: u64) -> Result<Answer> {
        if let Some(b) = &self.broken {
            return Err(Error::new(format!("connection is dead ({b}); dial again")));
        }
        if !matches!(req, Request::Hello(_)) {
            self.require_compatible()?;
        }
        let deadline = Instant::now() + self.timeout;
        self.reader.get_mut().deadline = deadline;
        self.writer.get_mut().deadline = deadline;
        let id = self.next_id;
        self.next_id += 1;
        let method = req.method();
        let target = self.target.clone();
        let io = |e: std::io::Error| {
            let kind = if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                "timed out".to_string()
            } else {
                e.to_string()
            };
            format!("{target} {method}: {kind}")
        };
        let sent = write_frame(&mut self.writer, Kind::Request, id, &msg::payload(req))
            .and_then(|()| self.writer.flush());
        if let Err(e) = sent {
            let m = io(e);
            self.broken = Some(m.clone());
            self.disconnected = true;
            return Err(Error::new(m));
        }
        let mut log = Vec::new();
        loop {
            let frame = match self.reader.read() {
                Ok(Some(f)) => f,
                Ok(None) => {
                    let m = format!("{target} {method}: the node closed the connection");
                    self.broken = Some(m.clone());
                    self.disconnected = true;
                    return Err(Error::new(m));
                }
                Err(e) => {
                    let m = io(e);
                    self.broken = Some(m.clone());
                    self.disconnected = true;
                    return Err(Error::new(m));
                }
            };
            if frame.id != id {
                return Err(self.poison(&format!(
                    "frame for request {} while waiting on {id}",
                    frame.id
                )));
            }
            match frame.kind {
                Kind::Response => return Ok(Answer::Response(frame.payload)),
                Kind::Error => {
                    let reply = pt_json::from_bytes::<ErrorReply>(&frame.payload)
                        .map_err(|e| self.poison(&format!("{method} error frame: {e}")))?;
                    return Err(Error::Node(reply));
                }
                Kind::Chunk => {
                    if log.len() as u64 + frame.payload.len() as u64 > log_max {
                        return Err(
                            self.poison(&format!("{method} sent more than {log_max} bytes"))
                        );
                    }
                    log.extend_from_slice(&frame.payload);
                }
                Kind::End => return Ok(Answer::Stream(log, frame.payload)),
                Kind::Request => return Err(self.poison("a request frame from the node")),
            }
        }
    }
}
