use std::io::{self, Write};

pub const MAX_PACKET: usize = 64 << 10;

pub struct Frame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub fn write_frame(writer: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    writer.write_all(&[kind])?;
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(payload)
}

pub fn read_frames(pending: &mut Vec<u8>, max: usize) -> io::Result<Vec<Frame>> {
    let mut frames = Vec::new();
    while let Some(frame) = take_frame(pending, max)? {
        frames.push(frame);
    }
    Ok(frames)
}

pub fn take_frame(pending: &mut Vec<u8>, max: usize) -> io::Result<Option<Frame>> {
    if pending.len() >= 5 {
        let len = u32::from_be_bytes(pending[1..5].try_into().unwrap()) as usize;
        if len > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DERP frame too large",
            ));
        }
        if pending.len() < 5 + len {
            return Ok(None);
        }
        let frame = Frame {
            kind: pending[0],
            payload: pending[5..5 + len].to_vec(),
        };
        pending.drain(..5 + len);
        return Ok(Some(frame));
    }
    Ok(None)
}
