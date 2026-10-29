## Context

The tunnel has three parts: `tunnel_core` (Rust, the crypto), the relay (Rust, a UDP server) and the
Android app (Kotlin, `VpnService`, calling `tunnel_core` over JNI). The relay keeps sessions only in
memory, clients are anonymous, and the client pins the server's public keys at build time.

The first version derived every key from the INIT alone. That made the handshake one message long,
but it also meant that anything able to make a relay process the same INIT twice (an attacker
replaying it, or the client resending it to a restarted relay) got the same keys with fresh
counters.

## Goals / Non-Goals

**Goals:**

- No ChaCha20-Poly1305 key is ever used twice with the same nonce, across relay restarts included.
- No recorded packet is accepted twice, across repeated INITs and relay restarts included.
- No packet from the network can crash the relay.
- The tunnel heals on its own after a lost INIT, a relay restart or a network switch.
- A fresh clone either builds a working APK or fails the build with a message that says what to do.

**Non-Goals:**

- Forward secrecy against theft of the server's long-term key. The server random is sent in the
  clear, so whoever steals the server key can still derive old sessions. A server ephemeral key,
  as in PQConnect, is the fix; it is a larger protocol change.
- Client authentication, session expiry, forwarding to the internet, iOS.

## Decisions

### D1. The relay adds a fresh random to every session, sent in an ACK

The relay picks 32 random bytes per session it opens and sends them in an ACK. Traffic keys are
`HKDF(salt = random, ikm = base secret, info = "traffic")`, where the base secret comes from the
INIT as before. Two relay processes answering the same INIT pick different randoms and so derive
different keys, which is what makes counters restarting at 0 safe.

Alternatives considered:

- Random starting counters on the relay. Two processes could still overlap, and a new process whose
  counters start below the client's replay window would have every packet rejected.
- Persisting sessions on the relay. More code, and a crash between sending and saving would still
  reuse a nonce.

### D2. The ACK is authenticated with HMAC-SHA256, not the AEAD

A restarted relay signs a second ACK for the same INIT. With an AEAD key derived from the INIT, both
ACKs would use the same key and nonce, which leaks the Poly1305 key and allows forgeries. HMAC has
no nonce. The tag is truncated to 16 bytes and checked in constant time (`verify_truncated_left`).
The ACK also gives the client explicit proof that it talks to the real relay before it sends data.

### D3. A repeated INIT gets the original ACK

The relay keeps each session's ACK. When an INIT maps to a session id it already has, it sends that
ACK again and keeps the session as it is. A new session would start with an empty replay window and
accept recorded packets again.

### D4. The client remembers every server random it has used

On an ACK with a new random the client switches to the new keys. On an ACK with the current random
it does nothing (the relay answering a resent INIT). On an ACK with an older random it refuses:
switching back would reset the receive window under keys whose packets an attacker may have
recorded. The list grows by one entry per relay restart during one connection, which is small.

### D5. The client resends INIT every 10 seconds for the life of the connection

This covers both a lost INIT and a relay restart with one rule and no timers on the relay. It costs
one 1121-byte packet and one decapsulation on the relay per client every 10 seconds. The resend
thread copies the INIT bytes and never touches the native handle, so it cannot outlive it unsafely.

### D6. Cleanup uses the run's own objects, and stale runs cannot report

`stopTunnel()` closes the socket and interface through the service's fields and waits up to one
second. A slow old run used to close the fields' values, which by then could belong to the next
run. Each run now closes only the objects it created, and a run counter makes status messages from
an old run silent.

### D7. The key check runs in `preBuild`, not in the Copy task

Gradle skips a Copy task with no source files (`NO-SOURCE`), and its actions with it, so a check in
`copyServerKey.doLast` never ran on a fresh clone. `preBuild.doFirst` always runs.

### D8. The build looks for `cargo` in the usual rustup homes

An app started from the Dock gets the system PATH, which has no Rust. The `buildRust` task tries
`~/.cargo/bin` (the official installer) and `/opt/homebrew/opt/rustup/bin` (Homebrew's keg-only
rustup) before PATH, puts both on the child's PATH so `cargo ndk` is found, and passes the SDK
location so `cargo-ndk` finds the NDK.

## Risks / Trade-offs

- [The handshake is now a round trip] → One extra RTT before the first packet. Packets read from the
  interface before the ACK are dropped; the kernel and the apps retry them.
- [INIT resend costs relay CPU] → One ML-KEM decapsulation per client per 10 seconds. Fine for a demo;
  a real relay would cache INITs by hash or use a cookie.
- [A relay restart still loses up to 10 seconds of traffic] → Until the next INIT resend. A relay
  that answers unknown session ids with a hint would shorten this; not needed here.
