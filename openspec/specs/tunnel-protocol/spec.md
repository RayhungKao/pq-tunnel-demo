# tunnel-protocol Specification

## Purpose
How the client and the relay agree on keys and protect packets: a hybrid X25519 + ML-KEM-768 handshake against pinned server keys, a per-session server random confirmed by an ACK, ChaCha20-Poly1305 DATA packets and a replay window. Implemented in `core/src/lib.rs` (`tunnel_core`), shared by the relay and the Android app.

## Requirements

### Requirement: INIT against pinned server keys

The client SHALL start a connection from the server's pinned public keys (an X25519 key followed by
an ML-KEM-768 encapsulation key, 1216 bytes) by sending an INIT packet: type `1`, a fresh X25519
ephemeral public key (32 bytes) and an ML-KEM-768 ciphertext (1088 bytes).

#### Scenario: Malformed input is refused
- **WHEN** the server receives an INIT of the wrong length, or the client is given public keys of the
  wrong length
- **THEN** the call returns `BadLength` and creates no session

### Requirement: Handshake secrets need both X25519 and ML-KEM

Both sides SHALL derive a base secret, an ACK MAC key and an 8-byte session id with HKDF-SHA256 over
the X25519 shared secret followed by the ML-KEM shared secret, salt `pq-tunnel-demo v2`, and info
SHA-256(INIT body ‖ server public keys).

#### Scenario: Client and server agree
- **WHEN** a client connects with the server's public keys and the server accepts the INIT
- **THEN** both sides hold the same session id and can exchange DATA in both directions

#### Scenario: Keys saved to disk still work
- **WHEN** the server's keys are written with `to_bytes` and read back with `from_bytes`
- **THEN** the public keys are unchanged and a client handshake with the loaded keys succeeds

### Requirement: The server confirms each session with a fresh random

On every INIT it accepts, the server SHALL pick 32 random bytes and answer with an ACK packet: type
`3`, the session id, the random and the first 16 bytes of HMAC-SHA256 over (type ‖ session id ‖
random) under the ACK MAC key. The traffic keys SHALL be HKDF-SHA256 with the random as salt, the
base secret as input and info `traffic`, giving one ChaCha20-Poly1305 key per direction. The client
SHALL check the tag in constant time.

#### Scenario: No data before the ACK
- **WHEN** the client has sent INIT and no ACK has arrived
- **THEN** sealing a packet returns nothing and nothing is sent

#### Scenario: An ACK from another server is refused
- **WHEN** a server with different keys answers the client's INIT
- **THEN** the client refuses its ACK and still sends no data

#### Scenario: A forged ACK is refused
- **WHEN** any byte of the random in a valid ACK is changed
- **THEN** the client returns `Decrypt` and keeps its state

### Requirement: A restarted server never reuses a key and nonce

Two server processes that accept the same INIT SHALL derive different traffic keys. The client SHALL
switch to new keys when an ACK with a new random arrives, SHALL ignore an ACK with its current
random, and SHALL refuse with `Replay` an ACK whose random it has used before.

#### Scenario: Same INIT, new process, new keys
- **WHEN** a second server process with the same long-term keys accepts an INIT the first one already
  accepted
- **THEN** its first sealed packet differs from the first process's first sealed packet of the same
  plaintext
- **AND** a packet the client sealed under the first process's keys does not open

#### Scenario: The client follows the restart
- **WHEN** the client receives the second process's ACK
- **THEN** its next packets open on the second process

#### Scenario: An old ACK cannot switch the client back
- **WHEN** the first process's ACK is replayed after the client switched
- **THEN** the client returns `Replay` and keeps the second process's keys
- **AND** the second process's ACK received again is accepted without a change

### Requirement: Authenticated DATA packets

A DATA packet SHALL be type `2`, the session id (8 bytes), a big-endian 64-bit counter and the
ChaCha20-Poly1305 ciphertext of the IP packet. The nonce SHALL be four zero bytes followed by the
counter, and the 17-byte header SHALL be the associated data. Each side SHALL increase its send
counter by one per packet.

#### Scenario: Tampering is detected
- **WHEN** any byte of a sealed packet is changed
- **THEN** opening it returns `Decrypt`

#### Scenario: A forged packet does not block the real one
- **WHEN** a tampered copy of a packet is opened before the original
- **THEN** the original still opens

### Requirement: Replay window that tolerates reordering

The receiver SHALL accept each counter at most once, SHALL accept counters out of order within the
64 counters below the highest one seen, and SHALL refuse anything older with `Replay`. Only a packet
that authenticates SHALL move the window.

#### Scenario: Reordered packets are accepted, repeats are not
- **WHEN** packets 2, 0, 0, 1, 2 arrive in that order
- **THEN** 2, 0 and 1 open and both repeats return `Replay`

#### Scenario: Packets older than the window are refused
- **WHEN** a packet arrives after 64 newer packets have been accepted
- **THEN** it returns `Replay`
