//! Tunnel crypto: a one-message hybrid handshake (X25519 + ML-KEM-768) and an AEAD packet layer.
//!
//! The client knows the server's public keys in advance (pinned), sends one INIT packet, and both
//! sides derive two ChaCha20-Poly1305 keys from both shared secrets. An attacker has to break both
//! X25519 and ML-KEM to read the traffic, the same "hybrid" idea PQConnect uses.
//!
//! Packets:
//!   INIT: [1][client X25519 pub, 32][ML-KEM ciphertext, 1088]
//!   DATA: [2][session id, 8][counter, 8][ciphertext + 16-byte tag]

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Ciphertext, Encoded, EncodedSizeUser, KemCore, MlKem768};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

#[cfg(target_os = "android")]
mod android;

pub const INIT: u8 = 1;
pub const DATA: u8 = 2;

type Ek = <MlKem768 as KemCore>::EncapsulationKey;
type Dk = <MlKem768 as KemCore>::DecapsulationKey;

pub const X25519_LEN: usize = 32;
pub const EK_LEN: usize = 1184;
pub const DK_LEN: usize = 2400;
pub const CT_LEN: usize = 1088;
pub const INIT_LEN: usize = 1 + X25519_LEN + CT_LEN;
const HEADER_LEN: usize = 1 + 8 + 8;
pub const PUBLIC_LEN: usize = X25519_LEN + EK_LEN;
pub const SECRET_LEN: usize = X25519_LEN + DK_LEN;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BadLength,
    BadType,
    BadKey,
    Decrypt,
    Replay,
    UnknownSession,
}

/// The server's long-term keys. `to_bytes`/`from_bytes` are what the relay stores on disk.
pub struct ServerKeys {
    x25519: StaticSecret,
    dk: Dk,
    public: Vec<u8>,
}

impl ServerKeys {
    pub fn generate() -> Self {
        let (dk, ek) = MlKem768::generate(&mut OsRng);
        Self::assemble(StaticSecret::random_from_rng(OsRng), dk, &ek)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != SECRET_LEN {
            return Err(Error::BadLength);
        }
        let x: [u8; 32] = bytes[..X25519_LEN].try_into().unwrap();
        let dk = Dk::from_bytes(&Encoded::<Dk>::try_from(&bytes[X25519_LEN..]).map_err(|_| Error::BadKey)?);
        let ek = dk.encapsulation_key().clone();
        Ok(Self::assemble(StaticSecret::from(x), dk, &ek))
    }

    fn assemble(x25519: StaticSecret, dk: Dk, ek: &Ek) -> Self {
        let mut public = PublicKey::from(&x25519).as_bytes().to_vec();
        public.extend_from_slice(&ek.as_bytes());
        Self { x25519, dk, public }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.x25519.to_bytes().to_vec();
        out.extend_from_slice(&self.dk.as_bytes());
        out
    }

    /// What clients pin: X25519 public key followed by the ML-KEM encapsulation key.
    pub fn public(&self) -> &[u8] {
        &self.public
    }

    /// Answers an INIT packet with a session. The session id is derived from the packet, so the
    /// server needs no reply before the client can send data.
    pub fn accept(&self, init: &[u8]) -> Result<Session, Error> {
        if init.len() != INIT_LEN {
            return Err(Error::BadLength);
        }
        if init[0] != INIT {
            return Err(Error::BadType);
        }
        let eph: [u8; 32] = init[1..1 + X25519_LEN].try_into().unwrap();
        let x_ss = self.x25519.diffie_hellman(&PublicKey::from(eph));
        let ct = Ciphertext::<MlKem768>::try_from(&init[1 + X25519_LEN..]).map_err(|_| Error::BadKey)?;
        let k_ss = self.dk.decapsulate(&ct).map_err(|_| Error::BadKey)?;
        Ok(Session::derive(x_ss.as_bytes(), k_ss.as_slice(), &init[1..], &self.public, false))
    }
}

/// Starts a client session against pinned server public keys. Returns the session and the INIT
/// packet to send.
pub fn connect(server_public: &[u8]) -> Result<(Session, Vec<u8>), Error> {
    if server_public.len() != PUBLIC_LEN {
        return Err(Error::BadLength);
    }
    let server_x: [u8; 32] = server_public[..X25519_LEN].try_into().unwrap();
    let ek = Ek::from_bytes(&Encoded::<Ek>::try_from(&server_public[X25519_LEN..]).map_err(|_| Error::BadKey)?);

    let eph = StaticSecret::random_from_rng(OsRng);
    let x_ss = eph.diffie_hellman(&PublicKey::from(server_x));
    let (ct, k_ss) = ek.encapsulate(&mut OsRng).map_err(|_| Error::BadKey)?;

    let mut init = vec![INIT];
    init.extend_from_slice(PublicKey::from(&eph).as_bytes());
    init.extend_from_slice(&ct);
    let session = Session::derive(x_ss.as_bytes(), k_ss.as_slice(), &init[1..], server_public, true);
    Ok((session, init))
}

pub struct Session {
    pub id: [u8; 8],
    send: ChaCha20Poly1305,
    recv: ChaCha20Poly1305,
    send_counter: u64,
    window: ReplayWindow,
}

impl Session {
    fn derive(x_ss: &[u8], k_ss: &[u8], transcript: &[u8], server_public: &[u8], is_client: bool) -> Self {
        let ikm = [x_ss, k_ss].concat();
        // Binding the transcript and the server's keys stops a key-substitution attacker from
        // making two different handshakes agree on one key.
        let info = Sha256::new().chain_update(transcript).chain_update(server_public).finalize();
        let mut okm = [0u8; 72];
        Hkdf::<Sha256>::new(Some(b"pq-tunnel-demo v1"), &ikm)
            .expand(&info, &mut okm)
            .expect("72 bytes is a valid HKDF length");
        let c2s = ChaCha20Poly1305::new(Key::from_slice(&okm[..32]));
        let s2c = ChaCha20Poly1305::new(Key::from_slice(&okm[32..64]));
        let (send, recv) = if is_client { (c2s, s2c) } else { (s2c, c2s) };
        Self { id: okm[64..].try_into().unwrap(), send, recv, send_counter: 0, window: ReplayWindow::default() }
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let counter = self.send_counter;
        // Counters never wrap in practice (2^64 packets); a real protocol rekeys long before.
        self.send_counter += 1;
        let mut packet = Vec::with_capacity(HEADER_LEN + plaintext.len() + 16);
        packet.push(DATA);
        packet.extend_from_slice(&self.id);
        packet.extend_from_slice(&counter.to_be_bytes());
        let ct = self
            .send
            .encrypt(&nonce(counter), Payload { msg: plaintext, aad: &packet })
            .expect("encryption of an in-memory buffer cannot fail");
        packet.extend_from_slice(&ct);
        packet
    }

    pub fn open(&mut self, packet: &[u8]) -> Result<Vec<u8>, Error> {
        if packet.len() < HEADER_LEN + 16 {
            return Err(Error::BadLength);
        }
        if packet[0] != DATA {
            return Err(Error::BadType);
        }
        if packet[1..9] != self.id {
            return Err(Error::UnknownSession);
        }
        let counter = u64::from_be_bytes(packet[9..HEADER_LEN].try_into().unwrap());
        if !self.window.check(counter) {
            return Err(Error::Replay);
        }
        let plain = self
            .recv
            .decrypt(&nonce(counter), Payload { msg: &packet[HEADER_LEN..], aad: &packet[..HEADER_LEN] })
            .map_err(|_| Error::Decrypt)?;
        // Only an authentic packet may move the window, or a forged one could block real traffic.
        self.window.mark(counter);
        Ok(plain)
    }
}

/// Reads the session id of a DATA packet, so the relay can find the session to open it with.
pub fn session_id(packet: &[u8]) -> Option<[u8; 8]> {
    (packet.len() >= HEADER_LEN && packet[0] == DATA).then(|| packet[1..9].try_into().unwrap())
}

fn nonce(counter: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_be_bytes());
    *Nonce::from_slice(&n)
}

/// 64-packet sliding window: UDP may reorder packets, so "strictly increasing" would drop real
/// traffic. Same idea as IPsec and WireGuard.
#[derive(Default)]
struct ReplayWindow {
    /// One past the highest counter seen, so 0 means "nothing yet".
    top: u64,
    /// Bit i set = counter (top - 1 - i) was seen.
    bits: u64,
}

impl ReplayWindow {
    fn check(&self, c: u64) -> bool {
        if c >= self.top {
            return true;
        }
        let age = self.top - 1 - c;
        age < 64 && self.bits & (1 << age) == 0
    }

    fn mark(&mut self, c: u64) {
        if c >= self.top {
            let shift = c + 1 - self.top;
            self.bits = if shift >= 64 { 0 } else { self.bits << shift };
            self.bits |= 1;
            self.top = c + 1;
        } else {
            self.bits |= 1 << (self.top - 1 - c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Session, Session) {
        let server = ServerKeys::generate();
        let (client, init) = connect(server.public()).unwrap();
        (client, server.accept(&init).unwrap())
    }

    #[test]
    fn both_directions_round_trip() {
        let (mut c, mut s) = pair();
        assert_eq!(c.id, s.id);
        assert_eq!(s.open(&c.seal(b"ping")).unwrap(), b"ping");
        assert_eq!(c.open(&s.seal(b"pong")).unwrap(), b"pong");
    }

    #[test]
    fn keys_survive_save_and_load() {
        let server = ServerKeys::generate();
        let loaded = ServerKeys::from_bytes(&server.to_bytes()).unwrap();
        assert_eq!(server.public(), loaded.public());
        let (mut c, init) = connect(server.public()).unwrap();
        let mut s = loaded.accept(&init).unwrap();
        assert_eq!(s.open(&c.seal(b"hi")).unwrap(), b"hi");
    }

    #[test]
    fn wrong_server_cannot_read() {
        let (mut c, init) = connect(ServerKeys::generate().public()).unwrap();
        let mut other = ServerKeys::generate().accept(&init).unwrap();
        assert_ne!(other.open(&c.seal(b"secret")), Ok(b"secret".to_vec()));
    }

    #[test]
    fn tampering_is_rejected() {
        let (mut c, mut s) = pair();
        let mut p = c.seal(b"data");
        *p.last_mut().unwrap() ^= 1;
        assert_eq!(s.open(&p), Err(Error::Decrypt));
        // A forged packet must not burn its counter: the real one still opens.
        p.last_mut().map(|b| *b ^= 1);
        assert_eq!(s.open(&p).unwrap(), b"data");
    }

    #[test]
    fn replays_are_rejected_but_reordering_is_fine() {
        let (mut c, mut s) = pair();
        let p: Vec<_> = (0..3).map(|i| c.seal(&[i])).collect();
        assert_eq!(s.open(&p[2]).unwrap(), [2]);
        assert_eq!(s.open(&p[0]).unwrap(), [0]);
        assert_eq!(s.open(&p[0]), Err(Error::Replay));
        assert_eq!(s.open(&p[1]).unwrap(), [1]);
        assert_eq!(s.open(&p[2]), Err(Error::Replay));
    }

    #[test]
    fn packets_older_than_the_window_are_rejected() {
        let (mut c, mut s) = pair();
        let old = c.seal(b"old");
        for _ in 0..64 {
            s.open(&c.seal(b"x")).unwrap();
        }
        assert_eq!(s.open(&old), Err(Error::Replay));
    }

    #[test]
    fn malformed_init_is_rejected() {
        let server = ServerKeys::generate();
        assert_eq!(server.accept(&[INIT; 10]).err(), Some(Error::BadLength));
        assert_eq!(connect(&[0; 5]).err(), Some(Error::BadLength));
    }
}
