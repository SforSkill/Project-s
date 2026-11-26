//! Replay container: `PSRP` magic, format version, a header blob, then frames.
//!
//! ```text
//! "PSRP" | u32 version | u32 header_len | header | { u32 len | u32 fnv1a(payload) | payload }*
//! ```
//! Frames are self-checking, so a file cut by a crash is read up to the last
//! complete frame. Payload encoding belongs to the caller (the game records its
//! simulation inputs, see `game_sim::record`).

use std::io::{self, Read, Write};

pub const MAGIC: [u8; 4] = *b"PSRP";
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("not a Projet S replay")]
    BadMagic,
    #[error("unsupported replay format version {0}")]
    Version(u32),
    #[error("frame too large")]
    TooLarge,
}

pub fn fnv1a(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in data {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

pub struct ReplayWriter<W: Write> {
    out: W,
    pub frames: u64,
    pub bytes: u64,
}

impl<W: Write> ReplayWriter<W> {
    pub fn new(mut out: W, header: &[u8]) -> Result<Self, ReplayError> {
        out.write_all(&MAGIC)?;
        out.write_all(&FORMAT_VERSION.to_le_bytes())?;
        out.write_all(&(header.len() as u32).to_le_bytes())?;
        out.write_all(header)?;
        Ok(Self {
            out,
            frames: 0,
            bytes: 12 + header.len() as u64,
        })
    }

    pub fn frame(&mut self, payload: &[u8]) -> Result<(), ReplayError> {
        if payload.len() > MAX_FRAME {
            return Err(ReplayError::TooLarge);
        }
        self.out.write_all(&(payload.len() as u32).to_le_bytes())?;
        self.out.write_all(&fnv1a(payload).to_le_bytes())?;
        self.out.write_all(payload)?;
        self.frames += 1;
        self.bytes += 8 + payload.len() as u64;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), ReplayError> {
        self.out.flush()?;
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.out
    }
}

pub struct ReplayReader<R: Read> {
    input: R,
    pub header: Vec<u8>,
    /// Set when the file ended in the middle of a frame or a frame was corrupt.
    pub truncated: bool,
}

impl<R: Read> ReplayReader<R> {
    pub fn open(mut input: R) -> Result<Self, ReplayError> {
        let mut m = [0u8; 4];
        input.read_exact(&mut m)?;
        if m != MAGIC {
            return Err(ReplayError::BadMagic);
        }
        let mut b = [0u8; 4];
        input.read_exact(&mut b)?;
        let v = u32::from_le_bytes(b);
        if v != FORMAT_VERSION {
            return Err(ReplayError::Version(v));
        }
        input.read_exact(&mut b)?;
        let n = u32::from_le_bytes(b) as usize;
        if n > MAX_FRAME {
            return Err(ReplayError::TooLarge);
        }
        let mut header = vec![0u8; n];
        input.read_exact(&mut header)?;
        Ok(Self {
            input,
            header,
            truncated: false,
        })
    }

    /// Next complete, valid frame; `None` at the end (or at a damaged tail).
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        let mut h = [0u8; 8];
        match read_full(&mut self.input, &mut h) {
            Ok(0) => return None,
            Ok(8) => {}
            _ => {
                self.truncated = true;
                return None;
            }
        }
        let len = u32::from_le_bytes(h[..4].try_into().ok()?) as usize;
        let sum = u32::from_le_bytes(h[4..].try_into().ok()?);
        if len > MAX_FRAME {
            self.truncated = true;
            return None;
        }
        let mut p = vec![0u8; len];
        if read_full(&mut self.input, &mut p).ok()? != len || fnv1a(&p) != sum {
            self.truncated = true;
            return None;
        }
        Some(p)
    }
}

fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_truncation() {
        let mut w = ReplayWriter::new(Vec::new(), b"hdr").unwrap();
        w.frame(b"one").unwrap();
        w.frame(b"two").unwrap();
        let bytes = w.into_inner();
        let mut r = ReplayReader::open(&bytes[..]).unwrap();
        assert_eq!(r.header, b"hdr");
        assert_eq!(r.next_frame().unwrap(), b"one");
        assert_eq!(r.next_frame().unwrap(), b"two");
        assert!(r.next_frame().is_none() && !r.truncated);
        // Cut in the middle of the last frame.
        let cut = &bytes[..bytes.len() - 2];
        let mut r = ReplayReader::open(cut).unwrap();
        assert_eq!(r.next_frame().unwrap(), b"one");
        assert!(r.next_frame().is_none() && r.truncated);
        // Corrupted payload.
        let mut bad = bytes.clone();
        let l = bad.len();
        bad[l - 1] ^= 0xff;
        let mut r = ReplayReader::open(&bad[..]).unwrap();
        r.next_frame();
        assert!(r.next_frame().is_none() && r.truncated);
        assert!(matches!(
            ReplayReader::open(&b"XXXX0000"[..]),
            Err(ReplayError::BadMagic)
        ));
    }
}
