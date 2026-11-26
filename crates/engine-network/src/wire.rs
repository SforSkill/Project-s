//! Little-endian byte writer/reader with LEB128 varints.

use crate::NetError;

#[derive(Default, Debug, Clone)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            buf: Vec::with_capacity(n),
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn var(&mut self, mut v: u64) -> &mut Self {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(b);
                return self;
            }
            self.buf.push(b | 0x80);
        }
    }

    /// Zig-zag signed varint.
    pub fn svar(&mut self, v: i64) -> &mut Self {
        self.var(((v << 1) ^ (v >> 63)) as u64)
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }

    /// Length-prefixed bytes.
    pub fn blob(&mut self, b: &[u8]) -> &mut Self {
        self.var(b.len() as u64);
        self.bytes(b)
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.blob(s.as_bytes())
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], NetError> {
        if self.remaining() < n {
            return Err(NetError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], NetError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8, NetError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, NetError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, NetError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64, NetError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16, NetError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, NetError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32, NetError> {
        let v = f32::from_le_bytes(self.array()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(NetError::Malformed("non-finite float"))
        }
    }

    pub fn f64(&mut self) -> Result<f64, NetError> {
        let v = f64::from_le_bytes(self.array()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(NetError::Malformed("non-finite float"))
        }
    }

    pub fn var(&mut self) -> Result<u64, NetError> {
        let mut v = 0u64;
        for i in 0..10 {
            let b = self.u8()?;
            v |= ((b & 0x7f) as u64) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(NetError::Malformed("varint too long"))
    }

    pub fn svar(&mut self) -> Result<i64, NetError> {
        let u = self.var()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    /// Length-prefixed bytes, at most `max` long.
    pub fn blob(&mut self, max: usize) -> Result<&'a [u8], NetError> {
        let n = self.var()? as usize;
        if n > max {
            return Err(NetError::Malformed("blob too long"));
        }
        self.take(n)
    }

    pub fn str(&mut self, max: usize) -> Result<String, NetError> {
        let b = self.blob(max)?;
        std::str::from_utf8(b)
            .map(|s| s.to_string())
            .map_err(|_| NetError::Malformed("invalid utf-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut w = Writer::new();
        w.u8(7)
            .u16(65000)
            .u32(123456)
            .var(300)
            .svar(-5)
            .svar(i64::MIN)
            .str("héllo")
            .f32(1.5);
        let b = w.finish();
        let mut r = Reader::new(&b);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u16().unwrap(), 65000);
        assert_eq!(r.u32().unwrap(), 123456);
        assert_eq!(r.var().unwrap(), 300);
        assert_eq!(r.svar().unwrap(), -5);
        assert_eq!(r.svar().unwrap(), i64::MIN);
        assert_eq!(r.str(16).unwrap(), "héllo");
        assert_eq!(r.f32().unwrap(), 1.5);
        assert!(r.is_empty());
        assert!(r.u8().is_err());
    }
}
