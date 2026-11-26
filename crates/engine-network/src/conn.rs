//! An established, encrypted connection: sequence numbers, acks, RTT, replay
//! protection and the four channels.
//!
//! | Channel   | Delivery                                   | Used for                         |
//! |-----------|--------------------------------------------|----------------------------------|
//! | Input     | unreliable, latest wins                     | user commands (sent redundantly) |
//! | Snapshot  | unreliable                                 | world state (delta vs acked)     |
//! | Urgent    | unreliable, repeated until acked, deduped  | shots, damage, kills             |
//! | Reliable  | reliable, ordered                          | match state, chat, smoke volumes |

use crate::crypto::{Cipher, SessionKeys, TAG_LEN};
use crate::wire::{Reader, Writer};
use crate::{DATA, NetError};
use std::collections::{BTreeMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Input,
    Snapshot,
    Urgent,
    Reliable,
}

/// Maximum UDP payload we emit (fits in common MTUs with IPv6 + tunnel overhead).
pub const MAX_PACKET: usize = 1200;
pub const HEADER_LEN: usize = 1 + 4 + 8;
/// Largest message accepted on any channel.
pub const MAX_MESSAGE: usize = MAX_PACKET - HEADER_LEN - TAG_LEN - 32;
pub const TIMEOUT: f64 = 10.0;
const KEEPALIVE: f64 = 0.1;
const URGENT_REPEATS: u8 = 3;

const K_INPUT: u8 = 1;
const K_SNAPSHOT: u8 = 2;
const K_URGENT: u8 = 3;
const K_RELIABLE: u8 = 4;
const K_DISCONNECT: u8 = 7;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_lost: u64,
    pub packets_acked: u64,
    pub rejected: u64,
    pub reliable_resends: u64,
    /// Smoothed round-trip time and deviation (seconds).
    pub srtt: f64,
    pub rttvar: f64,
    /// Latest RTT sample.
    pub last_rtt: f64,
}

impl ConnStats {
    pub fn loss_ratio(&self) -> f64 {
        let total = self.packets_acked + self.packets_lost;
        if total == 0 {
            0.0
        } else {
            self.packets_lost as f64 / total as f64
        }
    }
}

struct SentPacket {
    seq: u64,
    time: f64,
    reliable: Vec<u32>,
    urgent: Vec<u32>,
}

struct OutReliable {
    id: u32,
    data: Vec<u8>,
    last_sent: Option<f64>,
}

struct OutUrgent {
    id: u32,
    data: Vec<u8>,
    sends: u8,
}

pub struct Connection {
    pub conn_id: u32,
    is_client: bool,
    tx: Cipher,
    rx: Cipher,
    next_seq: u64,
    // Receive side.
    max_recv: u64,
    recv_window: u128,
    any_received: bool,
    // Send side.
    sent: VecDeque<SentPacket>,
    pending_unreliable: Vec<(u8, Vec<u8>)>,
    reliable_out: VecDeque<OutReliable>,
    next_reliable: u32,
    reliable_expected: u32,
    reliable_buffer: BTreeMap<u32, Vec<u8>>,
    urgent_out: Vec<OutUrgent>,
    next_urgent: u32,
    urgent_seen: HashSet<u32>,
    urgent_seen_order: VecDeque<u32>,
    last_send: f64,
    pub last_recv: f64,
    ack_dirty: bool,
    pub stats: ConnStats,
    pub closed: Option<String>,
    disconnect_out: Option<String>,
}

impl Connection {
    pub fn new(conn_id: u32, keys: &SessionKeys, is_client: bool, now: f64) -> Connection {
        let (tx, rx) = if is_client {
            (
                Cipher::new(&keys.client_to_server, 1),
                Cipher::new(&keys.server_to_client, 2),
            )
        } else {
            (
                Cipher::new(&keys.server_to_client, 2),
                Cipher::new(&keys.client_to_server, 1),
            )
        };
        Connection {
            conn_id,
            is_client,
            tx,
            rx,
            // Sequence 0 is used by the handshake confirmation.
            next_seq: 1,
            max_recv: 0,
            recv_window: 0,
            any_received: false,
            sent: VecDeque::new(),
            pending_unreliable: Vec::new(),
            reliable_out: VecDeque::new(),
            next_reliable: 0,
            reliable_expected: 0,
            reliable_buffer: BTreeMap::new(),
            urgent_out: Vec::new(),
            next_urgent: 0,
            urgent_seen: HashSet::new(),
            urgent_seen_order: VecDeque::new(),
            last_send: now,
            last_recv: now,
            ack_dirty: false,
            stats: ConnStats {
                srtt: 0.1,
                rttvar: 0.05,
                last_rtt: 0.1,
                ..Default::default()
            },
            closed: None,
            disconnect_out: None,
        }
    }

    pub fn is_client(&self) -> bool {
        self.is_client
    }

    /// Smoothed round-trip time (RFC 6298 SRTT), seconds.
    pub fn rtt(&self) -> f64 {
        self.stats.srtt
    }

    pub fn send(&mut self, ch: Channel, data: Vec<u8>) -> Result<(), NetError> {
        if data.len() > MAX_MESSAGE {
            return Err(NetError::TooLarge(data.len()));
        }
        match ch {
            Channel::Input => self.pending_unreliable.push((K_INPUT, data)),
            Channel::Snapshot => self.pending_unreliable.push((K_SNAPSHOT, data)),
            Channel::Urgent => {
                let id = self.next_urgent;
                self.next_urgent = self.next_urgent.wrapping_add(1);
                self.urgent_out.push(OutUrgent { id, data, sends: 0 });
            }
            Channel::Reliable => {
                let id = self.next_reliable;
                self.next_reliable = self.next_reliable.wrapping_add(1);
                self.reliable_out.push_back(OutReliable {
                    id,
                    data,
                    last_sent: None,
                });
            }
        }
        Ok(())
    }

    /// Queues a disconnect notice (sent with the next flush).
    pub fn disconnect(&mut self, reason: &str) {
        self.disconnect_out = Some(reason.chars().take(120).collect());
    }

    /// Number of reliable messages not acknowledged yet (backpressure).
    pub fn reliable_backlog(&self) -> usize {
        self.reliable_out.len()
    }

    fn resend_timeout(&self) -> f64 {
        (self.stats.srtt * 1.25 + 4.0 * self.stats.rttvar).clamp(0.05, 1.0)
    }

    fn header(&self, seq: u64) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[0] = DATA;
        h[1..5].copy_from_slice(&self.conn_id.to_le_bytes());
        h[5..13].copy_from_slice(&seq.to_le_bytes());
        h
    }

    /// Builds the datagrams to send now. `force` sends at least one packet
    /// (keepalive / ack) even if nothing is queued.
    pub fn flush(&mut self, now: f64, force: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let rto = self.resend_timeout();
        // Collect what goes out this time.
        let mut items: Vec<(u8, u32, Vec<u8>)> = Vec::new();
        if let Some(r) = self.disconnect_out.take() {
            let mut w = Writer::new();
            w.str(&r);
            items.push((K_DISCONNECT, 0, w.finish()));
        }
        for u in &mut self.urgent_out {
            if u.sends < URGENT_REPEATS {
                items.push((K_URGENT, u.id, u.data.clone()));
                u.sends += 1;
            }
        }
        self.urgent_out.retain(|u| u.sends < URGENT_REPEATS);
        for (k, d) in self.pending_unreliable.drain(..) {
            items.push((k, 0, d));
        }
        let mut resent = 0;
        for r in &mut self.reliable_out {
            let due = r.last_sent.is_none_or(|t| now - t >= rto);
            if due {
                if r.last_sent.is_some() {
                    resent += 1;
                }
                r.last_sent = Some(now);
                items.push((K_RELIABLE, r.id, r.data.clone()));
            }
        }
        self.stats.reliable_resends += resent;
        if items.is_empty() && !force && !self.ack_dirty && now - self.last_send < KEEPALIVE {
            return out;
        }
        // Pack into packets.
        let mut i = 0;
        loop {
            let mut body = Writer::with_capacity(MAX_PACKET);
            body.var(self.max_recv).u64(self.ack_bits());
            let mut rel = Vec::new();
            let mut urg = Vec::new();
            while i < items.len() {
                let (k, id, ref d) = items[i];
                let need = 1 + 5 + 3 + d.len();
                if body.len() + need > MAX_PACKET - HEADER_LEN - TAG_LEN && body.len() > 12 {
                    break;
                }
                body.u8(k);
                if k == K_URGENT || k == K_RELIABLE {
                    body.var(id as u64);
                }
                body.blob(d);
                if k == K_RELIABLE {
                    rel.push(id);
                }
                if k == K_URGENT {
                    urg.push(id);
                }
                i += 1;
            }
            let seq = self.next_seq;
            self.next_seq += 1;
            let h = self.header(seq);
            let sealed = self.tx.seal(seq, &h, &body.buf);
            let mut pkt = Vec::with_capacity(HEADER_LEN + sealed.len());
            pkt.extend_from_slice(&h);
            pkt.extend_from_slice(&sealed);
            self.stats.packets_sent += 1;
            self.stats.bytes_sent += pkt.len() as u64;
            self.sent.push_back(SentPacket {
                seq,
                time: now,
                reliable: rel,
                urgent: urg,
            });
            out.push(pkt);
            if i >= items.len() {
                break;
            }
        }
        // Forget very old packets (counted lost).
        while self.sent.len() > 2048 {
            self.sent.pop_front();
            self.stats.packets_lost += 1;
        }
        self.last_send = now;
        self.ack_dirty = false;
        out
    }

    fn ack_bits(&self) -> u64 {
        // Bit i = packet max_recv - 1 - i received.
        (self.recv_window >> 1) as u64
    }

    /// Replay window: true if `seq` is new (and records it).
    fn accept_seq(&mut self, seq: u64) -> bool {
        if !self.any_received || seq > self.max_recv {
            let shift = if self.any_received {
                seq - self.max_recv
            } else {
                128
            };
            self.recv_window = if shift >= 128 {
                0
            } else {
                self.recv_window << shift
            };
            self.recv_window |= 1;
            self.max_recv = seq;
            self.any_received = true;
            return true;
        }
        let back = self.max_recv - seq;
        if back >= 128 {
            return false;
        }
        let bit = 1u128 << back;
        if self.recv_window & bit != 0 {
            return false;
        }
        self.recv_window |= bit;
        true
    }

    fn on_ack(&mut self, ack: u64, bits: u64, now: f64) {
        let acked = |seq: u64| -> bool {
            if seq == ack {
                return true;
            }
            if seq > ack {
                return false;
            }
            let back = ack - seq - 1;
            back < 64 && bits & (1u64 << back) != 0
        };
        let mut done_rel = Vec::new();
        let mut done_urg = Vec::new();
        let mut keep = VecDeque::with_capacity(self.sent.len());
        while let Some(p) = self.sent.pop_front() {
            if acked(p.seq) {
                self.stats.packets_acked += 1;
                if p.seq == ack {
                    let sample = (now - p.time).max(0.0);
                    self.stats.last_rtt = sample;
                    let err = (sample - self.stats.srtt).abs();
                    self.stats.rttvar = 0.75 * self.stats.rttvar + 0.25 * err;
                    self.stats.srtt = 0.875 * self.stats.srtt + 0.125 * sample;
                }
                done_rel.extend(p.reliable);
                done_urg.extend(p.urgent);
            } else if ack >= 64 && p.seq < ack - 64 {
                // Out of the ack window: lost.
                self.stats.packets_lost += 1;
            } else {
                keep.push_back(p);
            }
        }
        self.sent = keep;
        if !done_rel.is_empty() {
            self.reliable_out.retain(|r| !done_rel.contains(&r.id));
        }
        if !done_urg.is_empty() {
            self.urgent_out.retain(|u| !done_urg.contains(&u.id));
        }
    }

    /// Processes one DATA datagram. Returns the messages delivered.
    pub fn receive(&mut self, pkt: &[u8], now: f64) -> Result<Vec<(Channel, Vec<u8>)>, NetError> {
        if pkt.len() < HEADER_LEN + TAG_LEN || pkt[0] != DATA {
            return Err(NetError::Truncated);
        }
        let seq = u64::from_le_bytes(pkt[5..13].try_into().expect("8 bytes"));
        let plain = match self.rx.open(seq, &pkt[..HEADER_LEN], &pkt[HEADER_LEN..]) {
            Ok(p) => p,
            Err(e) => {
                self.stats.rejected += 1;
                return Err(e);
            }
        };
        if seq == 0 {
            // Handshake confirmation duplicate.
            return Ok(Vec::new());
        }
        if !self.accept_seq(seq) {
            self.stats.rejected += 1;
            return Err(NetError::Replay);
        }
        self.stats.packets_received += 1;
        self.stats.bytes_received += pkt.len() as u64;
        self.last_recv = now;
        let mut r = Reader::new(&plain);
        let ack = r.var()?;
        let bits = r.u64()?;
        self.on_ack(ack, bits, now);
        let mut out = Vec::new();
        let mut ack_eliciting = false;
        while !r.is_empty() {
            let k = r.u8()?;
            match k {
                K_INPUT | K_SNAPSHOT => {
                    let d = r.blob(MAX_MESSAGE)?.to_vec();
                    out.push((
                        if k == K_INPUT {
                            Channel::Input
                        } else {
                            Channel::Snapshot
                        },
                        d,
                    ));
                }
                K_URGENT => {
                    ack_eliciting = true;
                    let id = r.var()? as u32;
                    let d = r.blob(MAX_MESSAGE)?;
                    if self.urgent_seen.insert(id) {
                        self.urgent_seen_order.push_back(id);
                        if self.urgent_seen_order.len() > 4096
                            && let Some(old) = self.urgent_seen_order.pop_front()
                        {
                            self.urgent_seen.remove(&old);
                        }
                        out.push((Channel::Urgent, d.to_vec()));
                    }
                }
                K_RELIABLE => {
                    ack_eliciting = true;
                    let id = r.var()? as u32;
                    let d = r.blob(MAX_MESSAGE)?;
                    let ahead = id.wrapping_sub(self.reliable_expected);
                    if ahead < 1 << 20 && self.reliable_buffer.len() < 4096 {
                        self.reliable_buffer.entry(id).or_insert_with(|| d.to_vec());
                    }
                    while let Some(d) = self.reliable_buffer.remove(&self.reliable_expected) {
                        out.push((Channel::Reliable, d));
                        self.reliable_expected = self.reliable_expected.wrapping_add(1);
                    }
                }
                K_DISCONNECT => {
                    let reason = r.str(256)?;
                    self.closed = Some(reason);
                }
                _ => return Err(NetError::Malformed("unknown message kind")),
            }
        }
        if ack_eliciting {
            self.ack_dirty = true;
        }
        Ok(out)
    }

    pub fn timed_out(&self, now: f64) -> bool {
        now - self.last_recv > TIMEOUT
    }

    /// Seals the handshake confirmation (sequence 0) for the ACCEPT packet.
    pub fn seal_confirm(&self, aad: &[u8]) -> Vec<u8> {
        self.tx.seal(0, aad, b"projet-s ok")
    }

    pub fn open_confirm(&self, aad: &[u8], sealed: &[u8]) -> Result<(), NetError> {
        let p = self.rx.open(0, aad, sealed)?;
        if p == b"projet-s ok" {
            Ok(())
        } else {
            Err(NetError::Handshake("bad confirmation"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_keys;

    fn pair() -> (Connection, Connection) {
        let keys = derive_keys(&[7; 32], &[1; 16], &[2; 16], 5);
        (
            Connection::new(5, &keys, true, 0.0),
            Connection::new(5, &keys, false, 0.0),
        )
    }

    #[test]
    fn channels_and_acks() {
        let (mut c, mut s) = pair();
        c.send(Channel::Reliable, b"r1".to_vec()).unwrap();
        c.send(Channel::Reliable, b"r2".to_vec()).unwrap();
        c.send(Channel::Input, b"in".to_vec()).unwrap();
        c.send(Channel::Urgent, b"shot".to_vec()).unwrap();
        let pkts = c.flush(0.0, false);
        assert_eq!(pkts.len(), 1);
        let msgs = s.receive(&pkts[0], 0.01).unwrap();
        assert_eq!(msgs.len(), 4);
        // Replay is rejected.
        assert!(matches!(s.receive(&pkts[0], 0.02), Err(NetError::Replay)));
        // Ack back.
        let back = s.flush(0.02, true);
        c.receive(&back[0], 0.05).unwrap();
        assert_eq!(c.reliable_backlog(), 0);
        assert!((c.stats.last_rtt - 0.05).abs() < 1e-9);
    }

    #[test]
    fn reliable_survives_loss_and_reorder() {
        let (mut c, mut s) = pair();
        for i in 0..5u8 {
            c.send(Channel::Reliable, vec![i]).unwrap();
            let _lost = c.flush(i as f64 * 0.01, false);
        }
        // All lost: after the timeout they are resent together.
        let pkts = c.flush(1.0, false);
        let mut got = Vec::new();
        for p in pkts.iter().rev() {
            got.extend(s.receive(p, 1.01).unwrap().into_iter().map(|m| m.1[0]));
        }
        assert_eq!(got, vec![0, 1, 2, 3, 4]);
        assert!(c.stats.reliable_resends >= 5);
    }

    #[test]
    fn urgent_is_repeated_and_deduplicated() {
        let (mut c, mut s) = pair();
        c.send(Channel::Urgent, b"kill".to_vec()).unwrap();
        let p1 = c.flush(0.0, false);
        let p2 = c.flush(0.001, true);
        let a = s.receive(&p2[0], 0.01).unwrap();
        let b = s.receive(&p1[0], 0.011).unwrap();
        assert_eq!(a.len() + b.len(), 1, "delivered once");
    }

    #[test]
    fn large_queue_splits_packets() {
        let (mut c, mut s) = pair();
        for _ in 0..10 {
            c.send(Channel::Reliable, vec![9; 500]).unwrap();
        }
        let pkts = c.flush(0.0, false);
        assert!(pkts.len() >= 5);
        assert!(pkts.iter().all(|p| p.len() <= MAX_PACKET));
        let n: usize = pkts.iter().map(|p| s.receive(p, 0.0).unwrap().len()).sum();
        assert_eq!(n, 10);
        assert!(c.send(Channel::Reliable, vec![0; MAX_MESSAGE + 1]).is_err());
    }
}
