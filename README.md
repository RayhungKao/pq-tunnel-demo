# pq-tunnel-demo

A small Android VPN app whose tunnel crypto is written in Rust: a hybrid **X25519 + ML-KEM-768**
handshake and ChaCha20-Poly1305 packets, called from Kotlin over JNI. It is a study of the pieces a
mobile [PQConnect](https://www.pqconnect.net) client needs (a system VPN interface, a Rust core
behind FFI, post-quantum key exchange), not a replacement for it.

```
 Android phone                                              relay (Rust, UDP :51820)
┌───────────────────────────────────────┐                ┌──────────────────────────┐
│ ping 10.66.0.1                        │                │ ServerKeys::accept(INIT) │
│   │ kernel routes 10.66.0.0/24 to tun0 │   encrypted    │ Session::open(DATA)      │
│   ▼                                   │   UDP          │   → log the IP packet    │
│ TunnelService (VpnService, Kotlin)    │ ─────────────▶ │   → answer pings to      │
│   │ JNI                               │ ◀───────────── │     10.66.0.1            │
│   ▼                                   │                └──────────────────────────┘
│ tunnel_core (Rust, libtunnel_core.so) │
└───────────────────────────────────────┘
```

## Layout

| Path | What it is |
|---|---|
| `core/` | Rust crate `tunnel_core`: handshake, AEAD packets, replay window, JNI bindings (`android.rs`) |
| `relay/` | Rust UDP server: key generation, session table, ICMP echo for the gateway |
| `android/` | Kotlin app: `VpnService`, a one-screen UI, Gradle tasks that cross-compile the Rust core |

## Protocol

The client pins the server's public keys (X25519 key + ML-KEM-768 encapsulation key) and sends one
packet; there is no round trip before data flows.

```
INIT  [0x01][client ephemeral X25519 pub, 32][ML-KEM-768 ciphertext, 1088]
DATA  [0x02][session id, 8][counter, 8][ChaCha20-Poly1305 ciphertext + tag]
```

- Keys: `HKDF-SHA256(salt = "pq-tunnel-demo v1", ikm = X25519 secret ‖ ML-KEM secret,
  info = SHA-256(INIT body ‖ server public keys))` gives one key per direction and the session id.
  Reading traffic requires breaking **both** X25519 and ML-KEM.
- The header is authenticated as AEAD associated data; the counter is the nonce.
- A 64-packet sliding window rejects replays but accepts UDP reordering. Only authentic packets
  move the window, so forged packets cannot block real ones.
- Sessions are keyed by id, not by IP address, and the relay replies to the latest source address,
  so a phone that changes networks keeps its session.

### Known limits (on purpose, for a demo)

- No forward secrecy against theft of the server's long-term key: the server contributes no
  ephemeral key. A real protocol (PQConnect included) adds one and rekeys periodically.
- The client is anonymous and the server is authenticated only implicitly: an impostor cannot
  derive the keys, but the client learns that only when the first reply arrives.
- The relay forwards nothing to the internet; it only answers pings to 10.66.0.1. A real relay
  writes packets to a TUN device and NATs them.
- Server keys reach the app at build time (`keys/server.pub` → APK asset), where PQConnect
  publishes them in DNS.

## Run it

Needs Rust (with `aarch64-linux-android` and `x86_64-linux-android` targets), `cargo-ndk`,
JDK 17, and the Android SDK with NDK 27.

```bash
cargo test
```

```bash
cargo run -p relay -- keygen keys
```

```bash
cargo run -p relay -- serve keys 0.0.0.0:51820
```

```bash
cd android && ./gradlew installDebug
```

Open **PQ Tunnel Demo**, keep the default relay `10.0.2.2:51820` (the host machine as seen from
the emulator), tap **Connect**, allow the VPN, then ping the gateway from the device:

```bash
adb shell ping -c 3 10.66.0.1
```

The relay logs each decrypted packet, and the app shows packets sent and received. On a real
phone, use the LAN address of the machine running the relay.

## Tests

`cargo test` runs 10 tests: round trips in both directions, key save/load, wrong-server and
tampered packets, replay and reordering, the window edge, malformed handshakes, ICMP checksums,
and one end-to-end ping over real UDP sockets between a client session and the relay loop.

The Android app was checked on an API 32 arm64 emulator: connect, three pings answered through
the tunnel, disconnect removes `tun0`.

## Next

- iOS: the same `tunnel_core` behind a Swift `NEPacketTunnelProvider` (needs a paid Apple
  developer account for the Network Extension entitlement on devices).
- Server ephemeral keys and rekeying; key discovery over DNS.

## License

MIT
