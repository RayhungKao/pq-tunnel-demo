## Why

The first version of the demo worked once, on one emulator, in the happy path. A review and an audit
of that commit found that it breaks in ways a reader of the code would find quickly:

- one 8-byte packet crashes the relay, and anyone can send it, because clients are anonymous
- replaying a recorded INIT resets a session's replay window, so recorded traffic can be replayed
- after a relay restart the same INIT gives the same keys with counters back at 0, so the relay
  encrypts new packets under a key and nonce it has already used, which breaks ChaCha20-Poly1305
- a fresh clone builds an APK without the server key, and the app crashes on Connect
- the Android client gives up on the first failed send, never resends a lost INIT, can close a
  newer connection from an older thread, and crashes on a mistyped relay address

This is a portfolio piece for a post-quantum VPN job. Its crypto has to hold up when someone reads
it, and it has to run when someone clones it. Writing the behavior down as requirements first means
each fix is checked against a scenario, not against a reading of the old code.

## What Changes

- **BREAKING** (wire format): the handshake gains an ACK from the relay. The ACK carries 32 fresh
  random bytes and an HMAC that proves the relay holds the server's private keys. Traffic keys mix
  in that random, so each relay process derives new keys even for a repeated INIT. INIT and DATA
  keep their layout; the key derivation label moves from `v1` to `v2`.
- The client sends no data before the ACK, switches to new keys when a new ACK arrives (a relay
  restart), and rejects an ACK it has used before.
- The relay answers a repeated INIT with the ACK it sent the first time, instead of opening a new
  session.
- The relay checks the IPv4 header length before reading header fields.
- The Android client resends INIT every 10 seconds, drops a packet when a send fails instead of
  closing the tunnel, validates the relay address, cleans up only its own socket and interface,
  and redraws packet counts once a second instead of on every packet.
- The Android build fails with a hint when `keys/server.pub` is missing, and finds `cargo` when
  Android Studio is started from the Dock.

## Capabilities

### New Capabilities

- `tunnel-protocol`: the handshake, key derivation, packet format and replay protection in
  `tunnel_core`, shared by the relay and the app.
- `relay`: key generation, the session table, and how the relay answers INIT and DATA.
- `android-client`: the VPN service, its connection lifecycle, the screen, and the build.

### Modified Capabilities

None. The repository had no specs before this change.

## Impact

- Code: `core/src/lib.rs`, `core/src/android.rs`, `relay/src/main.rs`,
  `android/app/build.gradle.kts`, `TunnelService.kt`, `MainActivity.kt`, `Native.kt`.
- New dependency: `hmac` 0.12 (RustCrypto; already pulled in by `hkdf`).
- Wire format: old clients cannot talk to the new relay. There are no deployed clients.
- Out of scope: forward secrecy against theft of the server's long-term key, client
  authentication, forwarding traffic to the internet, and an iOS client.
