## Why

A review and an audit after `harden-tunnel` found that the Android client tells the user the wrong
thing and that its connection lifecycle had no tests:

- the screen said "Connected" as soon as the VPN interface existed, before the relay had answered,
  so with the relay down it said "Connected" forever
- an error such as an unresolvable host was replaced by "Disconnected" within a millisecond, so the
  user never saw why the connection failed
- an IPv6 relay address was refused as a bad address
- a fresh clone waited for the whole Rust cross-compile before failing on the missing server key
- nothing tested that the native session is freed only after both packet loops end

Writing the first instrumented tests found three more bugs, all in stopping a connection:

- a read blocked on `tun0` does not return when another thread closes it, so Disconnect on an idle
  tunnel hung until the next packet happened to arrive, and the native session stayed allocated
- a Disconnect sent before the new connection had opened its socket was lost, and the connection
  kept running
- Disconnect followed quickly by Connect stopped the new connection too, because `stopSelf()`
  without a start id stops the service even when a newer start request is pending

## What Changes

- The status is "Waiting for the relay" until the relay's ACK arrives, then "Connected". JNI gains
  `Native.ready(handle)`, backed by `Client::is_ready()`.
- An error that ends a connection stays on screen instead of "Disconnected".
- Relay addresses are parsed by a pure function, `parseRelay`, which accepts `[ipv6]`,
  `[ipv6]:port` and a bare IPv6 address.
- The server key check runs in `buildRust.doFirst`, before any Rust is compiled.
- The sender loop waits on `tun0` with `Os.poll` and a 500 ms timeout and ends once the socket is
  closed.
- Each connection is a `Run` object with its own socket, interface and `stopped` flag; a stop that
  arrives before the socket or interface exists is seen when they are opened.
- `ACTION_STOP` calls `stopSelf(startId)`.
- New tests: JVM unit tests for `parseRelay`, and instrumented tests that drive the real service
  against a fake relay on the device.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `android-client`: status reflects the relay's ACK, errors stay visible, IPv6 addresses, stopping
  is reliable, the key check comes first, counts are verified.
- `tunnel-protocol`: the client reports whether it is ready to send.

## Impact

- Code: `TunnelService.kt`, new `RelayAddress.kt`, `Native.kt`, `core/src/android.rs`,
  `core/src/lib.rs`, `android/app/build.gradle.kts`.
- New test sources: `android/app/src/test/` (JVM) and `android/app/src/androidTest/` (device).
- New test dependencies: `junit` 4.13.2, `androidx.test:runner` 1.6.2, `androidx.test.ext:junit`
  1.2.1. No runtime dependencies.
- Wire format unchanged.
