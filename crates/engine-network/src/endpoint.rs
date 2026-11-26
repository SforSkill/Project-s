//! Handshake state machines (sans-IO): the server endpoint manages many
//! connections keyed by address; the client endpoint manages one.
//!
//! ```text
//! client                                   server
//!   HELLO  (pub_c, nonce_c, padded 512 B) ->
//!                                       <- CHALLENGE (nonce_c, cookie)      stateless, small
//!   CONNECT(pub_c, nonce_c, cookie, padded) ->
//!                                       <- ACCEPT (id, nonce_c, pub_s, nonce_s, sealed confirm)
//!   DATA ... (both ways, ChaCha20-Poly1305)
//! ```
//! The cookie (keyed PRF over address, key and time bucket) stops spoofed
//! addresses from creating state; padding the client packets means the server
//! never answers with more bytes than it received (no amplification).

use crate::conn::Connection;
use crate::crypto::{KeyPair, derive_keys, prf16, random_bytes};
use crate::wire::{Reader, Writer};
use crate::{ACCEPT, CHALLENGE, CONNECT, DATA, HELLO, MAGIC, NetError, PROTOCOL_VERSION, REJECT};
use std::collections::HashMap;
use std::net::SocketAddr;

pub const HANDSHAKE_SIZE: usize = 512;
const COOKIE_BUCKET: f64 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Connected {
        addr: SocketAddr,
        conn_id: u32,
    },
    Message {
        conn_id: u32,
        channel: crate::Channel,
        data: Vec<u8>,
    },
    Disconnected {
        conn_id: u32,
        reason: String,
    },
}

pub struct ServerEndpoint {
    secret: [u8; 32],
    pub max_connections: usize,
    conns: HashMap<u32, (SocketAddr, Connection, Vec<u8>)>,
    by_addr: HashMap<SocketAddr, u32>,
    /// Handshakes in progress by (address, client nonce) → conn id (duplicate CONNECT).
    pending: HashMap<(SocketAddr, [u8; 16]), u32>,
    pub rejected_packets: u64,
}

fn cookie(
    secret: &[u8; 32],
    addr: SocketAddr,
    pubkey: &[u8; 32],
    nonce: &[u8; 16],
    bucket: u64,
) -> [u8; 16] {
    let mut w = Writer::new();
    w.str(&addr.to_string())
        .bytes(pubkey)
        .bytes(nonce)
        .u64(bucket);
    prf16(secret, &w.buf)
}

fn padded(mut w: Writer) -> Vec<u8> {
    w.buf.resize(HANDSHAKE_SIZE, 0);
    w.finish()
}

impl ServerEndpoint {
    pub fn new(max_connections: usize) -> ServerEndpoint {
        ServerEndpoint {
            secret: random_bytes(),
            max_connections,
            conns: HashMap::new(),
            by_addr: HashMap::new(),
            pending: HashMap::new(),
            rejected_packets: 0,
        }
    }

    pub fn connection(&self, id: u32) -> Option<&Connection> {
        self.conns.get(&id).map(|c| &c.1)
    }

    pub fn connection_mut(&mut self, id: u32) -> Option<&mut Connection> {
        self.conns.get_mut(&id).map(|c| &mut c.1)
    }

    pub fn addr_of(&self, id: u32) -> Option<SocketAddr> {
        self.conns.get(&id).map(|c| c.0)
    }

    pub fn connection_ids(&self) -> Vec<u32> {
        self.conns.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.conns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conns.is_empty()
    }

    /// Handles one datagram. Replies (handshake) are appended to `out`.
    pub fn receive(
        &mut self,
        from: SocketAddr,
        pkt: &[u8],
        now: f64,
        out: &mut Vec<(SocketAddr, Vec<u8>)>,
        events: &mut Vec<ServerEvent>,
    ) {
        if let Err(_e) = self.receive_inner(from, pkt, now, out, events) {
            self.rejected_packets += 1;
        }
    }

    fn receive_inner(
        &mut self,
        from: SocketAddr,
        pkt: &[u8],
        now: f64,
        out: &mut Vec<(SocketAddr, Vec<u8>)>,
        events: &mut Vec<ServerEvent>,
    ) -> Result<(), NetError> {
        let kind = *pkt.first().ok_or(NetError::Truncated)?;
        match kind {
            DATA => {
                if pkt.len() < 5 {
                    return Err(NetError::Truncated);
                }
                let id = u32::from_le_bytes(pkt[1..5].try_into().expect("4 bytes"));
                let (addr, conn, _) = self.conns.get_mut(&id).ok_or(NetError::UnknownConnection)?;
                let msgs = conn.receive(pkt, now)?;
                // Authenticated packet from a new address: NAT rebinding.
                if *addr != from {
                    self.by_addr.remove(addr);
                    *addr = from;
                    self.by_addr.insert(from, id);
                }
                for (channel, data) in msgs {
                    events.push(ServerEvent::Message {
                        conn_id: id,
                        channel,
                        data,
                    });
                }
                if let Some(reason) = conn.closed.clone() {
                    self.remove(id);
                    events.push(ServerEvent::Disconnected {
                        conn_id: id,
                        reason,
                    });
                }
                Ok(())
            }
            HELLO | CONNECT => {
                if pkt.len() < HANDSHAKE_SIZE {
                    return Err(NetError::Handshake("unpadded handshake"));
                }
                let mut r = Reader::new(&pkt[1..]);
                if r.array::<4>()? != MAGIC {
                    return Err(NetError::Handshake("bad magic"));
                }
                let version = r.u16()?;
                let client_pub: [u8; 32] = r.array()?;
                let client_nonce: [u8; 16] = r.array()?;
                if version != PROTOCOL_VERSION {
                    let mut w = Writer::new();
                    w.u8(REJECT)
                        .bytes(&client_nonce)
                        .str("version de protocole différente");
                    out.push((from, w.finish()));
                    return Ok(());
                }
                let bucket = (now / COOKIE_BUCKET).floor() as u64;
                if kind == HELLO {
                    let c = cookie(&self.secret, from, &client_pub, &client_nonce, bucket);
                    let mut w = Writer::new();
                    w.u8(CHALLENGE).bytes(&client_nonce).bytes(&c);
                    out.push((from, w.finish()));
                    return Ok(());
                }
                let got: [u8; 16] = r.array()?;
                let valid = got == cookie(&self.secret, from, &client_pub, &client_nonce, bucket)
                    || (bucket > 0
                        && got
                            == cookie(&self.secret, from, &client_pub, &client_nonce, bucket - 1));
                if !valid {
                    return Err(NetError::Handshake("bad cookie"));
                }
                // Duplicate CONNECT: resend the same ACCEPT.
                if let Some(id) = self.pending.get(&(from, client_nonce))
                    && let Some((_, _, accept)) = self.conns.get(id)
                {
                    out.push((from, accept.clone()));
                    return Ok(());
                }
                if self.conns.len() >= self.max_connections {
                    let mut w = Writer::new();
                    w.u8(REJECT).bytes(&client_nonce).str("serveur complet");
                    out.push((from, w.finish()));
                    return Ok(());
                }
                // A new handshake from an address replaces its old connection.
                if let Some(old) = self.by_addr.get(&from).copied() {
                    self.remove(old);
                    events.push(ServerEvent::Disconnected {
                        conn_id: old,
                        reason: "reconnexion".into(),
                    });
                }
                let kp = KeyPair::generate();
                let shared = kp.agree(&client_pub)?;
                let server_nonce: [u8; 16] = random_bytes();
                let mut id = u32::from_le_bytes(random_bytes());
                while id == 0 || self.conns.contains_key(&id) {
                    id = u32::from_le_bytes(random_bytes());
                }
                let keys = derive_keys(&shared, &client_nonce, &server_nonce, id);
                let conn = Connection::new(id, &keys, false, now);
                let mut w = Writer::new();
                w.u8(ACCEPT)
                    .u32(id)
                    .bytes(&client_nonce)
                    .bytes(&kp.public)
                    .bytes(&server_nonce);
                let sealed = conn.seal_confirm(&w.buf);
                w.bytes(&sealed);
                let accept = w.finish();
                out.push((from, accept.clone()));
                self.conns.insert(id, (from, conn, accept));
                self.by_addr.insert(from, id);
                self.pending.insert((from, client_nonce), id);
                events.push(ServerEvent::Connected {
                    addr: from,
                    conn_id: id,
                });
                Ok(())
            }
            _ => Err(NetError::Malformed("unknown packet type")),
        }
    }

    /// Flushes every connection and drops the ones that timed out.
    pub fn flush(
        &mut self,
        now: f64,
        out: &mut Vec<(SocketAddr, Vec<u8>)>,
        events: &mut Vec<ServerEvent>,
    ) {
        let mut dead = Vec::new();
        for (id, (addr, conn, _)) in &mut self.conns {
            for p in conn.flush(now, false) {
                out.push((*addr, p));
            }
            if conn.timed_out(now) {
                dead.push(*id);
            }
        }
        for id in dead {
            self.remove(id);
            events.push(ServerEvent::Disconnected {
                conn_id: id,
                reason: "délai dépassé".into(),
            });
        }
    }

    /// Flushes one connection immediately (urgent stream).
    pub fn flush_one(&mut self, id: u32, now: f64, out: &mut Vec<(SocketAddr, Vec<u8>)>) {
        if let Some((addr, conn, _)) = self.conns.get_mut(&id) {
            for p in conn.flush(now, false) {
                out.push((*addr, p));
            }
        }
    }

    /// Sends a disconnect notice and forgets the connection.
    pub fn kick(&mut self, id: u32, reason: &str, now: f64, out: &mut Vec<(SocketAddr, Vec<u8>)>) {
        if let Some((addr, conn, _)) = self.conns.get_mut(&id) {
            conn.disconnect(reason);
            for p in conn.flush(now, true) {
                out.push((*addr, p));
            }
        }
        self.remove(id);
    }

    fn remove(&mut self, id: u32) {
        if let Some((addr, _, _)) = self.conns.remove(&id) {
            self.by_addr.remove(&addr);
        }
        self.pending.retain(|_, v| *v != id);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientState {
    Hello,
    Connect { cookie: [u8; 16] },
    Connected,
    Rejected(String),
    TimedOut,
}

pub struct ClientEndpoint {
    kp: KeyPair,
    nonce: [u8; 16],
    pub state: ClientState,
    pub conn: Option<Connection>,
    started: f64,
    last_handshake: f64,
}

impl ClientEndpoint {
    pub fn new(now: f64) -> ClientEndpoint {
        ClientEndpoint {
            kp: KeyPair::generate(),
            nonce: random_bytes(),
            state: ClientState::Hello,
            conn: None,
            started: now,
            last_handshake: f64::NEG_INFINITY,
        }
    }

    fn hello(&self, kind: u8, cookie: Option<&[u8; 16]>) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(kind)
            .bytes(&MAGIC)
            .u16(PROTOCOL_VERSION)
            .bytes(&self.kp.public)
            .bytes(&self.nonce);
        if let Some(c) = cookie {
            w.bytes(c);
        }
        padded(w)
    }

    /// Datagrams to send now (handshake retries, data, keepalives).
    pub fn poll(&mut self, now: f64) -> Vec<Vec<u8>> {
        match &self.state {
            ClientState::Hello | ClientState::Connect { .. } => {
                if now - self.started > 10.0 {
                    self.state = ClientState::TimedOut;
                    return Vec::new();
                }
                if now - self.last_handshake < 0.25 {
                    return Vec::new();
                }
                self.last_handshake = now;
                let pkt = match &self.state {
                    ClientState::Hello => self.hello(HELLO, None),
                    ClientState::Connect { cookie } => self.hello(CONNECT, Some(cookie)),
                    _ => unreachable!(),
                };
                vec![pkt]
            }
            ClientState::Connected => {
                let conn = self.conn.as_mut().expect("connected");
                if conn.timed_out(now) {
                    self.state = ClientState::TimedOut;
                    return Vec::new();
                }
                conn.flush(now, false)
            }
            _ => Vec::new(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.state == ClientState::Connected
    }

    /// Handles a datagram from the server; returns the delivered messages.
    pub fn receive(
        &mut self,
        pkt: &[u8],
        now: f64,
    ) -> Result<Vec<(crate::Channel, Vec<u8>)>, NetError> {
        let kind = *pkt.first().ok_or(NetError::Truncated)?;
        let mut r = Reader::new(&pkt[1..]);
        match kind {
            CHALLENGE if self.state == ClientState::Hello => {
                let nonce: [u8; 16] = r.array()?;
                if nonce != self.nonce {
                    return Err(NetError::Handshake("nonce mismatch"));
                }
                let cookie = r.array()?;
                self.state = ClientState::Connect { cookie };
                self.last_handshake = f64::NEG_INFINITY;
                Ok(Vec::new())
            }
            ACCEPT if matches!(self.state, ClientState::Connect { .. }) => {
                let id = r.u32()?;
                let nonce: [u8; 16] = r.array()?;
                if nonce != self.nonce {
                    return Err(NetError::Handshake("nonce mismatch"));
                }
                let server_pub: [u8; 32] = r.array()?;
                let server_nonce: [u8; 16] = r.array()?;
                let head_len = 1 + 4 + 16 + 32 + 16;
                let shared = self.kp.agree(&server_pub)?;
                let keys = derive_keys(&shared, &self.nonce, &server_nonce, id);
                let conn = Connection::new(id, &keys, true, now);
                conn.open_confirm(&pkt[..head_len], &pkt[head_len..])?;
                self.conn = Some(conn);
                self.state = ClientState::Connected;
                Ok(Vec::new())
            }
            REJECT => {
                let nonce: [u8; 16] = r.array()?;
                if nonce == self.nonce && !self.is_connected() {
                    self.state = ClientState::Rejected(r.str(200).unwrap_or_default());
                }
                Ok(Vec::new())
            }
            DATA => {
                let conn = self.conn.as_mut().ok_or(NetError::UnknownConnection)?;
                let msgs = conn.receive(pkt, now)?;
                if let Some(reason) = conn.closed.clone() {
                    self.state = ClientState::Rejected(reason);
                }
                Ok(msgs)
            }
            _ => Ok(Vec::new()),
        }
    }
}
