//! Tunnel crypto: a hybrid handshake (X25519 + ML-KEM-768) and an AEAD packet layer.
//!
//! The client knows the server's public keys in advance (pinned) and sends INIT. Only the real
//! server can derive the shared secrets from it; it answers with ACK, which carries 32 fresh random
//! bytes and a MAC (a keyed checksum) that proves the server derived them. The traffic keys mix
//! both shared secrets with that random, so an attacker has to break both X25519 and ML-KEM (the
//! "hybrid" idea PQConnect uses), and every relay process derives new keys even for a repeated
//! INIT: a restarted relay never reuses a (key, nonce) pair, and old recordings cannot be replayed.
//!
//! Packets:
//!   INIT: [1][client X25519 pub, 32][ML-KEM ciphertext, 1088]
//!   DATA: [2][session id, 8][counter, 8][ciphertext + 16-byte tag]
//!   ACK:  [3][session id, 8][server random, 32][HMAC-SHA256 tag, 16]

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Ciphertext, Encoded, EncodedSizeUser, KemCore, MlKem768};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

#[cfg(target_os = "android")]
mod android;

pub const INIT: u8 = 1;
pub const DATA: u8 = 2;
pub const ACK: u8 = 3;

type Ek = <MlKem768 as KemCore>::EncapsulationKey;
type Dk = <MlKem768 as KemCore>::DecapsulationKey;

pub const X25519_LEN: usize = 32;
pub const EK_LEN: usize = 1184;
pub const DK_LEN: usize = 2400;
pub const CT_LEN: usize = 1088;
pub const INIT_LEN: usize = 1 + X25519_LEN + CT_LEN;
const ACK_LEN: usize = 1 + 8 + 32 + 16;
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
    /// DATA arrived before the server's ACK.
    NotReady,
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

    /// Answers an INIT packet with a new session and the ACK packet to send back.
    pub fn accept(&self, init: &[u8]) -> Result<(Session, Vec<u8>), Error> {
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
        let hs = Handshake::derive(x_ss.as_bytes(), k_ss.as_slice(), &init[1..], &self.public);

        let mut random = [0u8; 32];
        OsRng.fill_bytes(&mut random);
        let mut ack = vec![ACK];
        ack.extend_from_slice(&hs.id);
        ack.extend_from_slice(&random);
        ack.extend_from_slice(&hs.ack_mac(&random).finalize().into_bytes()[..16]);
        Ok((hs.session(&random, false), ack))
    }
}

/// Secrets both sides derive from one INIT. They never encrypt traffic themselves.
struct Handshake {
    base: [u8; 32],
    mac_key: [u8; 32],
    id: [u8; 8],
}

impl Handshake {
    fn derive(x_ss: &[u8], k_ss: &[u8], transcript: &[u8], server_public: &[u8]) -> Self {
        let ikm = [x_ss, k_ss].concat();
        // Binding the transcript and the server's keys stops a key-substitution attacker from
        // making two different handshakes agree on one key.
        let info = Sha256::new().chain_update(transcript).chain_update(server_public).finalize();
        let mut okm = [0u8; 72];
        Hkdf::<Sha256>::new(Some(b"pq-tunnel-demo v2"), &ikm)
            .expand(&info, &mut okm)
            .expect("72 bytes is a valid HKDF length");
        Self {
            base: okm[..32].try_into().unwrap(),
            mac_key: okm[32..64].try_into().unwrap(),
            id: okm[64..].try_into().unwrap(),
        }
    }

    /// HMAC, not the AEAD: a restarted relay signs a second ACK for the same INIT, and an AEAD key
    /// used twice with the same nonce would let an attacker forge tags.
    fn ack_mac(&self, random: &[u8; 32]) -> Hmac<Sha256> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.mac_key).expect("HMAC takes any key length");
        mac.update(&[ACK]);
        mac.update(&self.id);
        mac.update(random);
        mac
    }

    fn session(&self, random: &[u8; 32], is_client: bool) -> Session {
        let mut okm = [0u8; 64];
        Hkdf::<Sha256>::new(Some(random), &self.base)
            .expand(b"traffic", &mut okm)
            .expect("64 bytes is a valid HKDF length");
        let c2s = ChaCha20Poly1305::new(Key::from_slice(&okm[..32]));
        let s2c = ChaCha20Poly1305::new(Key::from_slice(&okm[32..]));
        let (send, recv) = if is_client { (c2s, s2c) } else { (s2c, c2s) };
        Session { id: self.id, send, recv, send_counter: 0, window: ReplayWindow::default() }
    }
}

/// Starts a client against pinned server public keys. Returns the client and the INIT packet,
/// which the caller sends (and may resend; the relay answers repeats with the same ACK).
pub fn connect(server_public: &[u8]) -> Result<(Client, Vec<u8>), Error> {
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
    let handshake = Handshake::derive(x_ss.as_bytes(), k_ss.as_slice(), &init[1..], server_public);
    Ok((Client { handshake, session: None, current: None, used: Vec::new() }, init))
}

pub struct Client {
    handshake: Handshake,
    session: Option<Session>,
    current: Option<[u8; 32]>,
    /// Server randoms already used. A replayed old ACK must not switch back to old keys, which
    /// would reopen packets recorded under them.
    used: Vec<[u8; 32]>,
}

impl Client {
    /// True once the server's ACK has arrived and data can flow.
    pub fn is_ready(&self) -> bool {
        self.session.is_some()
    }

    /// None until the server's ACK has arrived; the caller drops the packet.
    pub fn seal(&mut self, plaintext: &[u8]) -> Option<Vec<u8>> {
        self.session.as_mut().map(|s| s.seal(plaintext))
    }

    /// Handles a packet from the server: an ACK (re)keys the client and yields None, DATA yields
    /// the IP packet inside.
    pub fn receive(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        if packet.first() == Some(&ACK) {
            return self.accept_ack(packet).map(|_| None);
        }
        self.session.as_mut().ok_or(Error::NotReady)?.open(packet).map(Some)
    }

    fn accept_ack(&mut self, ack: &[u8]) -> Result<(), Error> {
        if ack.len() != ACK_LEN {
            return Err(Error::BadLength);
        }
        if ack[1..9] != self.handshake.id {
            return Err(Error::UnknownSession);
        }
        let random: [u8; 32] = ack[9..41].try_into().unwrap();
        self.handshake.ack_mac(&random).verify_truncated_left(&ack[41..]).map_err(|_| Error::Decrypt)?;
        if self.current == Some(random) {
            return Ok(()); // the relay answering a resent INIT
        }
        if self.used.contains(&random) {
            return Err(Error::Replay);
        }
        // A new random means a new relay process (a restart): switch to its keys.
        self.session = Some(self.handshake.session(&random, true));
        self.current = Some(random);
        self.used.push(random);
        Ok(())
    }
}

/// One direction pair of traffic keys plus counters.
pub struct Session {
    pub id: [u8; 8],
    send: ChaCha20Poly1305,
    recv: ChaCha20Poly1305,
    send_counter: u64,
    window: ReplayWindow,
}

impl Session {
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

    fn handshake(server: &ServerKeys) -> (Client, Session) {
        let (mut client, init) = connect(server.public()).unwrap();
        let (session, ack) = server.accept(&init).unwrap();
        assert_eq!(client.receive(&ack), Ok(None));
        assert!(client.is_ready());
        (client, session)
    }

    fn pair() -> (Client, Session) {
        handshake(&ServerKeys::generate())
    }

    #[test]
    fn both_directions_round_trip() {
        let (mut c, mut s) = pair();
        assert_eq!(s.open(&c.seal(b"ping").unwrap()).unwrap(), b"ping");
        assert_eq!(c.receive(&s.seal(b"pong")), Ok(Some(b"pong".to_vec())));
    }

    #[test]
    fn nothing_is_sent_before_the_ack() {
        let (mut c, _init) = connect(ServerKeys::generate().public()).unwrap();
        assert!(!c.is_ready());
        assert!(c.seal(b"early").is_none());
    }

    #[test]
    fn keys_survive_save_and_load() {
        let server = ServerKeys::generate();
        let loaded = ServerKeys::from_bytes(&server.to_bytes()).unwrap();
        assert_eq!(server.public(), loaded.public());
        let (mut c, mut s) = handshake(&loaded);
        assert_eq!(s.open(&c.seal(b"hi").unwrap()).unwrap(), b"hi");
    }

    #[test]
    fn ack_from_the_wrong_server_is_rejected() {
        let (mut c, init) = connect(ServerKeys::generate().public()).unwrap();
        let (_, ack) = ServerKeys::generate().accept(&init).unwrap();
        assert!(c.receive(&ack).is_err());
        assert!(!c.is_ready());
        assert!(c.seal(b"secret").is_none());
    }

    #[test]
    fn forged_ack_is_rejected() {
        let server = ServerKeys::generate();
        let (mut c, init) = connect(server.public()).unwrap();
        let (_, mut ack) = server.accept(&init).unwrap();
        ack[20] ^= 1; // inside the server random
        assert_eq!(c.receive(&ack), Err(Error::Decrypt));
    }

    #[test]
    fn restarted_server_derives_new_keys() {
        // Two relay processes with the same long-term key both answer the same INIT.
        let bytes = ServerKeys::generate().to_bytes();
        let (first, second) = (ServerKeys::from_bytes(&bytes).unwrap(), ServerKeys::from_bytes(&bytes).unwrap());
        let (mut c, init) = connect(first.public()).unwrap();
        let (mut s1, ack1) = first.accept(&init).unwrap();
        c.receive(&ack1).unwrap();
        let reply1 = s1.seal(b"before restart");
        let recorded = c.seal(b"recorded").unwrap();

        let (mut s2, ack2) = second.accept(&init).unwrap();
        c.receive(&ack2).unwrap();
        // Same counter 0, but a new key: no (key, nonce) pair is used twice.
        assert_ne!(s2.seal(b"before restart"), reply1);
        assert_eq!(s2.open(&recorded), Err(Error::Decrypt));
        assert_eq!(s2.open(&c.seal(b"after").unwrap()).unwrap(), b"after");
        // An attacker replaying the first ACK cannot switch the client back to the old keys.
        assert_eq!(c.receive(&ack1), Err(Error::Replay));
        assert_eq!(c.receive(&ack2), Ok(None));
    }

    #[test]
    fn tampering_is_rejected() {
        let (mut c, mut s) = pair();
        let mut p = c.seal(b"data").unwrap();
        *p.last_mut().unwrap() ^= 1;
        assert_eq!(s.open(&p), Err(Error::Decrypt));
        // A forged packet must not burn its counter: the real one still opens.
        *p.last_mut().unwrap() ^= 1;
        assert_eq!(s.open(&p).unwrap(), b"data");
    }

    #[test]
    fn replays_are_rejected_but_reordering_is_fine() {
        let (mut c, mut s) = pair();
        let p: Vec<_> = (0..3).map(|i| c.seal(&[i]).unwrap()).collect();
        assert_eq!(s.open(&p[2]).unwrap(), [2]);
        assert_eq!(s.open(&p[0]).unwrap(), [0]);
        assert_eq!(s.open(&p[0]), Err(Error::Replay));
        assert_eq!(s.open(&p[1]).unwrap(), [1]);
        assert_eq!(s.open(&p[2]), Err(Error::Replay));
    }

    #[test]
    fn packets_older_than_the_window_are_rejected() {
        let (mut c, mut s) = pair();
        let old = c.seal(b"old").unwrap();
        for _ in 0..64 {
            s.open(&c.seal(b"x").unwrap()).unwrap();
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
