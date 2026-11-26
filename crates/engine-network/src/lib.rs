//! Projet S transport: sans-IO encrypted UDP.
//!
//! The crate never touches sockets: callers feed received datagrams and send what
//! it returns. This keeps it testable with simulated links ([`linksim`]) and lets
//! the server, the client and the network lab share the exact same code.

pub mod conn;
pub mod crypto;
pub mod endpoint;
pub mod linksim;
pub mod wire;

pub use conn::{Channel, ConnStats, Connection, MAX_MESSAGE, MAX_PACKET};
pub use endpoint::{ClientEndpoint, ClientState, ServerEndpoint, ServerEvent};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAGIC: [u8; 4] = *b"PRJS";

pub(crate) const HELLO: u8 = 0x01;
pub(crate) const CHALLENGE: u8 = 0x02;
pub(crate) const CONNECT: u8 = 0x03;
pub(crate) const ACCEPT: u8 = 0x04;
pub(crate) const REJECT: u8 = 0x05;
pub(crate) const DATA: u8 = 0x10;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum NetError {
    #[error("truncated packet")]
    Truncated,
    #[error("malformed packet: {0}")]
    Malformed(&'static str),
    #[error("authentication failed")]
    Auth,
    #[error("replayed or too old packet")]
    Replay,
    #[error("unknown connection")]
    UnknownConnection,
    #[error("handshake: {0}")]
    Handshake(&'static str),
    #[error("message too large ({0} bytes)")]
    TooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn addr(p: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], p))
    }

    /// Full handshake through the server endpoint, then messages both ways.
    #[test]
    fn handshake_and_traffic() {
        let mut server = ServerEndpoint::new(4);
        let mut client = ClientEndpoint::new(0.0);
        let mut events = Vec::new();
        let mut t = 0.0;
        for _ in 0..10 {
            for p in client.poll(t) {
                let mut out = Vec::new();
                server.receive(addr(5000), &p, t, &mut out, &mut events);
                for (to, reply) in out {
                    assert_eq!(to, addr(5000));
                    assert!(reply.len() <= p.len(), "no amplification");
                    client.receive(&reply, t).unwrap();
                }
            }
            t += 0.3;
        }
        assert!(client.is_connected());
        let id = match events[0] {
            ServerEvent::Connected { conn_id, .. } => conn_id,
            _ => panic!("no connect event"),
        };
        client
            .conn
            .as_mut()
            .unwrap()
            .send(Channel::Reliable, b"join".to_vec())
            .unwrap();
        let mut out = Vec::new();
        for p in client.poll(t) {
            server.receive(addr(5000), &p, t, &mut out, &mut events);
        }
        assert!(events.iter().any(|e| matches!(e, ServerEvent::Message { conn_id, data, .. } if *conn_id == id && data == b"join")));
        server
            .connection_mut(id)
            .unwrap()
            .send(Channel::Snapshot, b"snap".to_vec())
            .unwrap();
        server.flush(t, &mut out, &mut events);
        let mut got = Vec::new();
        for (_, p) in out {
            if p[0] == DATA {
                got.extend(client.receive(&p, t).unwrap());
            }
        }
        assert!(
            got.iter()
                .any(|(c, d)| *c == Channel::Snapshot && d == b"snap")
        );
    }

    #[test]
    fn spoofed_or_garbage_packets_create_no_state() {
        let mut server = ServerEndpoint::new(4);
        let mut out = Vec::new();
        let mut events = Vec::new();
        server.receive(addr(1), &[0x03; 600], 0.0, &mut out, &mut events);
        server.receive(addr(1), &[0x10; 40], 0.0, &mut out, &mut events);
        server.receive(addr(1), &[0x01; 20], 0.0, &mut out, &mut events);
        assert!(server.is_empty());
        assert!(events.is_empty());
        assert_eq!(server.rejected_packets, 3);
    }
}
