//! Connect server-streaming envelopes and a bounded producer channel.
use crate::{
    encode_json, Code, DynamicMessage, MessageDescriptor, ReflectMessage, Result, RpcError,
};
use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, HeaderValue},
    response::Response,
};
use futures_core::Stream;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, Sleep},
};
pub const DEFAULT_STREAM_CAPACITY: usize = 8;
pub const DEFAULT_MAX_STREAM_MESSAGE_BYTES: usize = 4 << 20;
pub const FLAG_COMPRESSED: u8 = 1;
pub const FLAG_END_STREAM: u8 = 2;
pub type StreamTrailers = BTreeMap<String, Vec<String>>;
#[derive(Default, Serialize)]
struct EndStream {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    metadata: StreamTrailers,
}
pub fn encode_envelope(flags: u8, payload: &[u8]) -> Result<Bytes> {
    let length = u32::try_from(payload.len())
        .map_err(|_| RpcError::new(Code::ResourceExhausted, "stream message exceeds 4 GiB"))?;
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(flags);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(Bytes::from(frame))
}
pub fn decode_stream_request(
    descriptor: MessageDescriptor,
    bytes: &[u8],
    limit: usize,
) -> Result<DynamicMessage> {
    if bytes.len() < 5 {
        return Err(RpcError::invalid("truncated Connect request envelope"));
    }
    let flags = bytes[0];
    if flags & FLAG_COMPRESSED != 0 {
        return Err(RpcError::new(
            Code::Unimplemented,
            "stream compression is unsupported; supported encoding: identity",
        ));
    }
    if flags != 0 {
        return Err(RpcError::invalid(
            "request envelopes must be uncompressed data messages",
        ));
    }
    let length = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
    if length > limit {
        return Err(RpcError::new(
            Code::ResourceExhausted,
            "stream request message exceeds limit",
        ));
    }
    if bytes.len() != length + 5 {
        return Err(RpcError::invalid(
            "server streaming requires exactly one complete request envelope",
        ));
    }
    crate::decode_json(descriptor, &bytes[5..])
}
fn end_frame(result: Result<StreamTrailers>) -> Bytes {
    let end = match result {
        Ok(metadata) => EndStream {
            error: None,
            metadata,
        },
        Err(error) => EndStream {
            error: Some(error),
            metadata: StreamTrailers::new(),
        },
    };
    let bytes = serde_json::to_vec(&end).expect("Connect errors and trailers serialize");
    encode_envelope(FLAG_END_STREAM, &bytes).expect("Connect end frame fits uint32")
}
/// A cloned sender shares a bounded queue. `send` waits when the HTTP body is
/// backpressured. Cancellation closes pending sends and terminates the producer.
#[derive(Clone)]
pub struct StreamSender {
    tx: mpsc::Sender<Bytes>,
    output: MessageDescriptor,
    cancel: watch::Sender<bool>,
    max_message_bytes: usize,
    deadline_expired: Arc<AtomicBool>,
}
impl StreamSender {
    pub(crate) fn deadline_expired(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.deadline_expired)
    }
    pub async fn send(&self, message: DynamicMessage) -> Result<()> {
        if message.descriptor() != self.output {
            return Err(RpcError::new(
                Code::Internal,
                "stream handler returned the wrong protobuf message type",
            ));
        }
        let bytes = encode_json(&message)?;
        if bytes.len() > self.max_message_bytes {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "stream response message exceeds limit",
            ));
        }
        let frame = encode_envelope(0, &bytes)?;
        tokio::select! {biased;_=self.cancelled()=>Err(RpcError::new(Code::Canceled,"stream cancelled")),sent=self.tx.send(frame)=>sent.map_err(|_|RpcError::new(Code::Canceled,"stream receiver disconnected"))}
    }
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }
    pub async fn cancelled(&self) {
        let mut rx = self.cancel.subscribe();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}
struct StreamBody {
    rx: mpsc::Receiver<Bytes>,
    task: JoinHandle<Result<StreamTrailers>>,
    cancel: watch::Sender<bool>,
    deadline: Option<Pin<Box<Sleep>>>,
    finished: bool,
    completion: Option<Result<StreamTrailers>>,
    deadline_expired: Arc<AtomicBool>,
}
impl Stream for StreamBody {
    type Item = std::result::Result<Bytes, std::convert::Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        // The producer may have finished while the client was slow to poll
        // the body. Preserve its recorded outcome before checking the clock.
        if self.completion.is_none() {
            if let Poll::Ready(result) = Pin::new(&mut self.task).poll(cx) {
                self.completion = Some(result.unwrap_or_else(|_| {
                    Err(RpcError::new(
                        Code::Internal,
                        "stream producer terminated unexpectedly",
                    ))
                }));
                self.cancel.send_replace(true);
                self.rx.close();
            }
        }
        if self.deadline_expired.load(Ordering::SeqCst)
            && self.completion.as_ref().is_some_and(
                |result| matches!(result, Err(error) if error.code == Code::DeadlineExceeded),
            )
        {
            self.finished = true;
            return Poll::Ready(Some(Ok(end_frame(self.completion.take().unwrap()))));
        }
        if self.completion.is_none()
            && self
                .deadline
                .as_mut()
                .is_some_and(|sleep| sleep.as_mut().poll(cx).is_ready())
        {
            self.finished = true;
            self.deadline_expired.store(true, Ordering::SeqCst);
            self.cancel.send_replace(true);
            self.task.abort();
            self.rx.close();
            return Poll::Ready(Some(Ok(end_frame(Err(RpcError::new(
                Code::DeadlineExceeded,
                "stream deadline exceeded",
            ))))));
        }
        if let Poll::Ready(Some(frame)) = self.rx.poll_recv(cx) {
            return Poll::Ready(Some(Ok(frame)));
        }
        if let Some(result) = self.completion.take() {
            self.finished = true;
            return Poll::Ready(Some(Ok(end_frame(result))));
        }
        match Pin::new(&mut self.task).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                let result = result.unwrap_or_else(|_| {
                    Err(RpcError::new(
                        Code::Internal,
                        "stream producer terminated unexpectedly",
                    ))
                });
                self.cancel.send_replace(true);
                self.rx.close();
                self.completion = Some(result);
                // The producer can enqueue its last message between poll_recv
                // returning Pending and its task completing. Drain that race.
                if let Ok(frame) = self.rx.try_recv() {
                    return Poll::Ready(Some(Ok(frame)));
                }
                self.finished = true;
                Poll::Ready(Some(Ok(end_frame(self.completion.take().unwrap()))))
            }
        }
    }
}
impl Drop for StreamBody {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
        self.rx.close();
        self.task.abort();
    }
}
pub(crate) fn stream_response<F, Fut>(
    output: MessageDescriptor,
    capacity: usize,
    max_message_bytes: usize,
    deadline: Option<Instant>,
    producer: F,
) -> Response
where
    F: FnOnce(StreamSender) -> Fut + Send + 'static,
    Fut: Future<Output = Result<StreamTrailers>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel(capacity.max(1));
    let (cancel, cancellation) = watch::channel(false);
    let deadline_expired = Arc::new(AtomicBool::new(false));
    let sender = StreamSender {
        tx,
        output,
        cancel: cancel.clone(),
        max_message_bytes,
        deadline_expired: Arc::clone(&deadline_expired),
    };
    let task_cancel = cancel.clone();
    let task_deadline_expired = Arc::clone(&deadline_expired);
    let task = tokio::spawn(async move {
        let mut cancellation = cancellation;
        let producer = producer(sender);
        let result = match deadline {
            Some(at) => {
                let mark_deadline = async {
                    tokio::time::sleep_until(at).await;
                    task_deadline_expired.store(true, Ordering::SeqCst);
                };
                tokio::select! {
                    result=producer=>result,
                    _=cancellation.changed()=>Err(RpcError::new(Code::Canceled,"stream cancelled")),
                    _=mark_deadline=>Err(RpcError::new(Code::DeadlineExceeded,"stream deadline exceeded")),
                }
            }
            None => tokio::select! {
                result=producer=>result,
                _=cancellation.changed()=>Err(RpcError::new(Code::Canceled,"stream cancelled")),
            },
        };
        task_cancel.send_replace(true);
        result
    });
    let body = StreamBody {
        rx,
        task,
        cancel,
        deadline: deadline.map(|at| Box::pin(tokio::time::sleep_until(at))),
        finished: false,
        completion: None,
        deadline_expired,
    };
    response(Body::from_stream(body), HeaderMap::new())
}
fn response(body: Body, headers: HeaderMap) -> Response {
    let mut response = Response::new(body);
    *response.headers_mut() = headers;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/connect+json"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert("connect-protocol-version", HeaderValue::from_static("1"));
    response
}
pub(crate) fn error_response(error: RpcError) -> Response {
    let headers = (*error.headers).clone();
    response(Body::from(end_frame(Err(error))), headers)
}
