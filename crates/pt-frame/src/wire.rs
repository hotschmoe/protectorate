//! Blocking frame I/O over any `Read`/`Write`, kept apart from the pure codec.

use std::io::{self, Read, Write};

use crate::codec::{self, Decoder, Frame, Kind};

pub fn write_frame(w: &mut impl Write, kind: Kind, id: u64, payload: &[u8]) -> io::Result<()> {
    let bytes = codec::encode(kind, id, payload).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("encoding frame {id}: {e}"),
        )
    })?;
    w.write_all(&bytes)
}

pub struct FrameReader<R> {
    inner: R,
    decoder: Decoder,
    buf: Vec<u8>,
}

impl<R: Read> FrameReader<R> {
    /// `pending` holds bytes already read from `inner` that belong to the frame stream.
    pub fn new(inner: R, pending: &[u8]) -> FrameReader<R> {
        let mut decoder = Decoder::new();
        decoder.feed(pending);
        FrameReader {
            inner,
            decoder,
            buf: vec![0; 64 * 1024],
        }
    }

    /// The next frame, or `None` at a clean end of stream. A codec error or a stream that ends
    /// inside a frame is `InvalidData`.
    pub fn read(&mut self) -> io::Result<Option<Frame>> {
        loop {
            match self.decoder.next_frame() {
                Ok(Some(f)) => return Ok(Some(f)),
                Ok(None) => {}
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
            }
            let n = match self.inner.read(&mut self.buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if n == 0 {
                return match self.decoder.finish() {
                    Ok(()) => Ok(None),
                    Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                };
            }
            self.decoder.feed(&self.buf[..n]);
        }
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}
