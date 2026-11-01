//! The hub end of the node link: pt-frame requests over a stream the hub member dialed to
//! `<node>:7475`. One request at a time; every call has a deadline, and a connection that timed
//! out or broke protocol is dead (dial again).

use std::io::{BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use pt_frame::Kind;
use pt_frame::msg::{
    self, ErrorReply, FetchLog, Hello, HelloReply, Inventory, JobList, JobRef, JobReply, JobStatus,
    LogEnd, Presence, Request, Sample, SubmitJob,
};
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::FromJson;

use crate::{Error, Result};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Log bytes accepted per FetchLog beyond what was asked for, as slack for chunk framing.
const FETCH_SLACK: u64 = 64 << 10;

pub struct NodeClient {
    target: String,
    reader: FrameReader<UnixStream>,
    writer: BufWriter<UnixStream>,
    stream: UnixStream,
    next_id: u64,
    broken: Option<String>,
}

enum Answer {
    Response(Vec<u8>),
    Log(Vec<u8>, LogEnd),
}

impl NodeClient {
    pub(crate) fn new(target: &str, stream: UnixStream) -> Result<NodeClient> {
        let dup = |s: &UnixStream| {
            s.try_clone()
                .map_err(|e| Error::new(format!("{target}: {e}")))
        };
        let mut c = NodeClient {
            target: target.to_string(),
            reader: FrameReader::new(dup(&stream)?, &[]),
            writer: BufWriter::new(dup(&stream)?),
            stream,
            next_id: 1,
            broken: None,
        };
        c.set_timeout(DEFAULT_TIMEOUT)?;
        Ok(c)
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    /// The deadline for each read and write of a call.
    pub fn set_timeout(&mut self, t: Duration) -> Result<()> {
        self.stream
            .set_read_timeout(Some(t))
            .and_then(|()| self.stream.set_write_timeout(Some(t)))
            .map_err(|e| Error::new(format!("{}: {e}", self.target)))
    }

    pub fn hello(&mut self) -> Result<HelloReply> {
        self.typed(&Request::Hello(Hello {
            hub_version: Some(env!("CARGO_PKG_VERSION").into()),
        }))
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

    /// Up to `max` log bytes from `offset`, and the end frame saying where the next fetch starts.
    pub fn fetch_log(
        &mut self,
        job_id: &str,
        attempt: u64,
        offset: u64,
        max: u64,
    ) -> Result<(Vec<u8>, LogEnd)> {
        let req = Request::FetchLog(FetchLog {
            job_id: job_id.into(),
            attempt,
            offset,
            max,
        });
        match self.call(&req, max.saturating_add(FETCH_SLACK))? {
            Answer::Log(bytes, end) => Ok((bytes, end)),
            Answer::Response(_) => Err(self.poison("a response frame to FetchLog")),
        }
    }

    pub fn cancel_job(&mut self, job_id: &str, attempt: u64) -> Result<JobReply> {
        self.typed(&Request::CancelJob(JobRef {
            job_id: job_id.into(),
            attempt,
        }))
    }

    pub fn ack_collected(&mut self, job_id: &str, attempt: u64) -> Result<JobReply> {
        self.typed(&Request::AckCollected(JobRef {
            job_id: job_id.into(),
            attempt,
        }))
    }

    pub fn list_jobs(&mut self) -> Result<JobList> {
        self.typed(&Request::ListJobs)
    }

    fn typed<T: FromJson>(&mut self, req: &Request) -> Result<T> {
        let method = req.method();
        match self.call(req, 0)? {
            Answer::Response(payload) => pt_json::from_bytes(&payload)
                .map_err(|e| self.poison(&format!("{method} answer: {e}"))),
            Answer::Log(..) => Err(self.poison(&format!("log frames for {method}"))),
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
            return Err(Error::new(m));
        }
        let mut log = Vec::new();
        loop {
            let frame = match self.reader.read() {
                Ok(Some(f)) => f,
                Ok(None) => {
                    let m = format!("{target} {method}: the node closed the connection");
                    self.broken = Some(m.clone());
                    return Err(Error::new(m));
                }
                Err(e) => {
                    let m = io(e);
                    self.broken = Some(m.clone());
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
                            self.poison(&format!("{method} sent more than {log_max} log bytes"))
                        );
                    }
                    log.extend_from_slice(&frame.payload);
                }
                Kind::End => {
                    let end = pt_json::from_bytes::<LogEnd>(&frame.payload)
                        .map_err(|e| self.poison(&format!("{method} end frame: {e}")))?;
                    return Ok(Answer::Log(log, end));
                }
                Kind::Request => return Err(self.poison("a request frame from the node")),
            }
        }
    }
}
