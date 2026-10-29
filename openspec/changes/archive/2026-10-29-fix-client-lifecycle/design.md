## Context

`TunnelService` runs one connection on three threads: the sender loop (reads `tun0`, seals, sends
UDP), the receiver loop (reads UDP, opens, writes `tun0`) and the INIT resend loop. `stopTunnel()`
runs on the main thread, closes the connection's socket and interface, and waits up to one second.
The native session may be freed only after both packet loops have ended.

## Goals / Non-Goals

**Goals:**

- The status never claims more than the client knows.
- A stop always ends the connection, whatever state the connection is in, within about a second.
- The lifecycle has tests that fail when it breaks, including by crashing the process.

**Non-Goals:**

- A foreground notification, always-on VPN support, or surviving process death.
- Testing a full handshake on the device: the test cannot hold the server's private key.

## Decisions

### D1. "Connected" means the ACK arrived

The receiver loop already sees every ACK, because `Native.open` handles it and returns null. After a
null result it asks `Native.ready(handle)` once, and on the first true reports "Connected". Before
that the status is "Waiting for the relay". A relay restart sends a new ACK but does not change the
status, since the client was connected before and is again.

### D2. The run's error replaces "Disconnected"

`finally` reports `error ?: "Disconnected"`. A plain stop sets no error, so it still ends with
"Disconnected".

### D3. Poll `tun0` instead of blocking in `read()`

> **Correction (verify-client-fixes):** this cause was never reproduced and is wrong. The tun fd is non-blocking; the old loop spun on empty reads instead of blocking. See `openspec/changes/archive/*-verify-client-fixes/design.md` D1.

On Linux, closing a file descriptor does not wake a thread blocked reading it; the thread wakes on
the next packet, and an idle tunnel may get none. Alternatives:

- A wake-up pipe polled together with `tun0`: instant, but one more file descriptor per connection
  and a write from `stopTunnel()`.
- Non-blocking reads with a sleep: the same latency as a poll timeout, but wakes even when the
  stop flag cannot have changed.

`Os.poll` with a 500 ms timeout is the smallest change: two wake-ups a second while idle, and a stop
takes effect within half a second. `EINTR` is retried; any other poll error ends the loop.

### D4. One `Run` object per connection

Before this change the socket and interface lived in service fields, and the connection opened them
some milliseconds after it started. A stop in that window closed nothing. Each connection now owns a
`Run` with volatile `stopped`, `socket` and `tun`. `stop()` writes `stopped` and then reads the
other two; the connection writes each of them and then reads `stopped`. With volatile fields at
least one side sees the other's write, so either the stop closes the object or the connection sees
`stopped` and returns. The generation counter moves from the worker thread to `onStartCommand`, so a
new connection's id exists before its thread starts.

### D5. `stopSelf(startId)`

`stopSelf()` stops the service even when a newer start request is already queued, and `onDestroy()`
then stops the new connection. With the start id, Android keeps the service when a newer start
exists.

### D6. Instrumented tests against a fake relay

The test opens a UDP socket on the device and passes its address as the relay. It cannot answer
with an ACK, so it checks what the client does before one: the INIT arrives, the status stays
"Waiting", errors stay visible, and stopping works. The tests run in the app's process, so a native
crash from freeing the session too early ends the run as a failure. **Correction (verify-client-fixes):** no test drove
the receiver at stop time, so freeing early went unnoticed; the raw pointer was replaced by a handle
table instead. Consent comes from
`appops set ... ACTIVATE_VPN allow` plus `VpnService.prepare()`, as `MainActivity` does; without
`prepare()` Android does not treat the app as the prepared VPN and `establish()` returns null.

## Risks / Trade-offs

- [Up to 500 ms to stop] → `stopTunnel()` waits up to one second, so the main thread is never
  blocked longer than before.
- [The instrumented tests need a running emulator] → They are not part of `cargo test` or
  `./gradlew test`; README lists the command.
