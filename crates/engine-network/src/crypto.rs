//! Key exchange, key derivation and packet encryption.
//!
//! - X25519 ephemeral Diffie-Hellman on both sides (forward secrecy).
//! - HKDF-SHA256 derives one key per direction, salted with both nonces.
//! - ChaCha20-Poly1305 encrypts every data packet; the 64-bit sequence number
//!   builds the nonce (never reused for a key), the clear header is the AAD.
//!
//! The server is not authenticated by a long-term key in this alpha: a network
//! attacker able to intercept the handshake can impersonate the server
//! (documented in `docs/security.md`). Packets cannot be read, forged or
//! replayed by anyone who did not take part in the exchange.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::NetError;

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("OS random generator unavailable");
    b
}

pub struct KeyPair {
    secret: StaticSecret,
    pub public: [u8; 32],
}

impl KeyPair {
    pub fn generate() -> KeyPair {
        let secret = StaticSecret::from(random_bytes::<32>());
        let public = PublicKey::from(&secret).to_bytes();
        KeyPair { secret, public }
    }

    /// Shared secret; rejects low-order points (all-zero output).
    pub fn agree(&self, their_public: &[u8; 32]) -> Result<[u8; 32], NetError> {
        let s = self.secret.diffie_hellman(&PublicKey::from(*their_public));
        if !s.was_contributory() {
            return Err(NetError::Handshake("non-contributory key"));
        }
        Ok(s.to_bytes())
    }
}

/// Directional keys of a session.
pub struct SessionKeys {
    pub client_to_server: [u8; 32],
    pub server_to_client: [u8; 32],
}

pub fn derive_keys(
    shared: &[u8; 32],
    client_nonce: &[u8; 16],
    server_nonce: &[u8; 16],
    conn_id: u32,
) -> SessionKeys {
    let mut salt = [0u8; 36];
    salt[..16].copy_from_slice(client_nonce);
    salt[16..32].copy_from_slice(server_nonce);
    salt[32..].copy_from_slice(&conn_id.to_le_bytes());
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut c2s = [0u8; 32];
    let mut s2c = [0u8; 32];
    hk.expand(b"projet-s v1 c2s", &mut c2s)
        .expect("hkdf length");
    hk.expand(b"projet-s v1 s2c", &mut s2c)
        .expect("hkdf length");
    SessionKeys {
        client_to_server: c2s,
        server_to_client: s2c,
    }
}

/// Keyed PRF (HKDF-SHA256) used for stateless handshake cookies.
pub fn prf16(key: &[u8; 32], data: &[u8]) -> [u8; 16] {
    let hk = Hkdf::<Sha256>::new(Some(key), data);
    let mut out = [0u8; 16];
    hk.expand(b"projet-s cookie", &mut out)
        .expect("hkdf length");
    out
}

pub struct Cipher {
    aead: ChaCha20Poly1305,
    dir: u8,
}

pub const TAG_LEN: usize = 16;

impl Cipher {
    pub fn new(key: &[u8; 32], dir: u8) -> Cipher {
        Cipher {
            aead: ChaCha20Poly1305::new(&Key::from(*key)),
            dir,
        }
    }

    fn nonce(&self, seq: u64) -> Nonce {
        let mut n = [0u8; 12];
        n[0] = self.dir;
        n[4..].copy_from_slice(&seq.to_le_bytes());
        Nonce::from(n)
    }

    pub fn seal(&self, seq: u64, aad: &[u8], plain: &[u8]) -> Vec<u8> {
        self.aead
            .encrypt(&self.nonce(seq), Payload { msg: plain, aad })
            .expect("encryption cannot fail for bounded packets")
    }

    pub fn open(&self, seq: u64, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, NetError> {
        self.aead
            .decrypt(&self.nonce(seq), Payload { msg: sealed, aad })
            .map_err(|_| NetError::Auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_and_seal() {
        let a = KeyPair::generate();
        let b = KeyPair::generate();
        let sa = a.agree(&b.public).unwrap();
        let sb = b.agree(&a.public).unwrap();
        assert_eq!(sa, sb);
        let k = derive_keys(&sa, &[1; 16], &[2; 16], 9);
        assert_ne!(k.client_to_server, k.server_to_client);
        let tx = Cipher::new(&k.client_to_server, 1);
        let rx = Cipher::new(&k.client_to_server, 1);
        let sealed = tx.seal(5, b"hdr", b"hello");
        assert_eq!(rx.open(5, b"hdr", &sealed).unwrap(), b"hello");
        assert!(rx.open(6, b"hdr", &sealed).is_err(), "wrong nonce");
        assert!(
            rx.open(5, b"hdx", &sealed).is_err(),
            "header is authenticated"
        );
        let mut bad = sealed.clone();
        bad[0] ^= 1;
        assert!(rx.open(5, b"hdr", &bad).is_err(), "tamper detected");
        assert!(a.agree(&[0u8; 32]).is_err(), "low-order point rejected");
    }
}
