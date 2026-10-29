# android-client Specification

## Purpose
How the Android app (`android/`) runs the tunnel: the `VpnService` interface and routes, the connection lifecycle and self-healing, the one-screen UI, and the Gradle build that cross-compiles the Rust core and packages the server's public key.

## Requirements

### Requirement: Only the tunnel subnet goes through the VPN

The app SHALL create a VPN interface with address 10.66.0.2/32, a route for 10.66.0.0/24 only and an
MTU of 1280, and SHALL exclude its own UDP socket from the VPN with `protect()`. Traffic outside
10.66.0.0/24 SHALL stay on the phone's normal network.

#### Scenario: Connect and ping the gateway
- **WHEN** the user taps Connect with relay `10.0.2.2:51820` on the emulator and allows the VPN
- **THEN** `tun0` has 10.66.0.2 and `ping 10.66.0.1` is answered

### Requirement: Bad input never crashes the app

The app SHALL accept a relay address as `host`, `host:port`, `[ipv6]`, `[ipv6]:port` or a bare IPv6
address, with port 51820 by default. Anything else SHALL show "Bad relay address" and start nothing,
including an empty host, a colon without a port, a port that is not one to five plain digits or is
outside 1–65535, and two or more colons in text that is not hex digits and colons. A missing or
unreadable server key SHALL show "No valid server key in the APK".

#### Scenario: Port missing after the colon
- **WHEN** the user enters `10.0.2.2:` and taps Connect
- **THEN** the screen shows "Bad relay address: 10.0.2.2:" and the app keeps running

#### Scenario: IPv6 relay
- **WHEN** the relay address is `[::1]:<port>`
- **THEN** the INIT is sent to that IPv6 address and port

#### Scenario: Extra colons and odd ports
- **WHEN** the relay address is `10.0.2.2:51820:1`, `[10.0.2.2]:4000` or `10.0.2.2:+80`
- **THEN** it is refused as a bad relay address

### Requirement: The tunnel heals on its own

The app SHALL resend its INIT every 10 seconds for as long as the connection runs. A failed send
SHALL drop that one packet and keep the tunnel. Packets read before the first ACK SHALL be dropped.

#### Scenario: Relay restart
- **WHEN** the relay restarts while the app is connected
- **THEN** pings to 10.66.0.1 are answered again within about 10 seconds, with no action from the user

#### Scenario: Send failure while switching networks
- **WHEN** one send fails, for example with "Network unreachable"
- **THEN** the tunnel stays up and later packets are sent

### Requirement: One connection at a time, cleaned up by its owner

Starting a connection SHALL first stop the previous one. Each connection SHALL close only the socket
and interface it created and SHALL free its native session only after both packet loops have ended.
Status messages from a connection that a newer one replaced SHALL not be shown.

#### Scenario: Connect tapped twice
- **WHEN** the user taps Connect twice in a row with the relay running
- **THEN** one connection runs, the screen shows "Connected", and pings are answered

#### Scenario: Disconnect
- **WHEN** the user taps Disconnect
- **THEN** `tun0` is removed and the screen shows "Disconnected"

### Requirement: Stopping always works

A stop SHALL end the connection within about one second whatever state it is in, including before
it has opened its socket or interface, during the name lookup, and while no packet is flowing. A
stop during the name lookup SHALL not be followed by bringing up `tun0`. A Disconnect SHALL not
stop a Connect that was requested after it.

#### Scenario: Disconnect on an idle tunnel
- **WHEN** the tunnel has carried no packets for 3 seconds and the user disconnects
- **THEN** the screen shows "Disconnected" within 2 seconds

#### Scenario: Connect and Disconnect in quick succession
- **WHEN** Connect and Disconnect are sent ten times in a row, half of the time with no pause
  between a Connect and the next request
- **THEN** every Disconnect ends with "Disconnected", every Connect reaches "Waiting for the relay",
  and the process does not crash

#### Scenario: Disconnect during a slow name lookup
- **WHEN** the user disconnects while the relay name is still being looked up
- **THEN** `tun0` is never brought up and the screen ends with "Disconnected"

### Requirement: Packet counts without flooding the UI thread

Each packet loop SHALL count only its own direction. The screen SHALL redraw the counts once a
second while it is visible and SHALL redraw the status when it changes.

#### Scenario: Counts while pinging
- **WHEN** three pings are answered
- **THEN** after the next redraw, at most one second later, the screen shows at least 3 sent and 3
  received

### Requirement: A fresh clone builds a working APK or fails with a hint

The build SHALL fail before compiling any Rust when `keys/server.pub` is missing, with a message
naming the keygen command. It SHALL put only `server.pub` into the APK, never `server.key`. It SHALL
find `cargo` in `~/.cargo/bin` or `/opt/homebrew/opt/rustup/bin` when PATH has no Rust.

#### Scenario: No keys folder
- **WHEN** `./gradlew assembleDebug` runs in a clone without `keys/`
- **THEN** the build fails with "No keys/server.pub. Run `cargo run -p relay -- keygen keys`" before
  any Rust crate is compiled

#### Scenario: Started without Rust on PATH
- **WHEN** the build runs with PATH set to `/usr/bin:/bin:/usr/sbin:/sbin`
- **THEN** the Rust core is still cross-compiled and the APK is built

### Requirement: The status says whether the relay answered

After the VPN interface is up, the app SHALL show "Waiting for the relay at <host>:<port>" until the
relay's ACK arrives, and "Connected to <host>:<port> (X25519 + ML-KEM-768)" from then on. A datagram
that is not a valid ACK SHALL not change the status. When a connection ends with an error, the app
SHALL keep showing that error instead of "Disconnected".

#### Scenario: Relay not running
- **WHEN** the user taps Connect and nothing answers at the relay address
- **THEN** the screen shows "Waiting for the relay" and never "Connected"

#### Scenario: Relay starts later
- **WHEN** the relay starts while the app is waiting
- **THEN** within about 10 seconds, at the next INIT resend, the screen shows "Connected"

#### Scenario: A stray datagram
- **WHEN** 40 bytes that are not an ACK arrive from the relay's address
- **THEN** the screen still shows "Waiting for the relay"

#### Scenario: Unresolvable host
- **WHEN** the user connects to `relay.invalid:51820`
- **THEN** the screen shows an error starting with "Error: Unable to resolve host" and keeps
  showing it

### Requirement: A freed native session cannot be used

Every call into the Rust core with a session handle that was freed or never issued SHALL return
null or false and SHALL not read freed memory. Freeing twice, or freeing 0, SHALL do nothing. A
call already in progress when the session is freed SHALL finish with the session intact.

#### Scenario: A call after free
- **WHEN** a session is freed and its handle is used again
- **THEN** the lookup finds nothing

#### Scenario: Handles are not reused
- **WHEN** a session is freed and a new one is created
- **THEN** the new handle differs from the old one, and neither is 0

#### Scenario: A call in progress during free
- **WHEN** a call holds the session and the session is freed
- **THEN** the call still sees the session until it returns

### Requirement: An idle tunnel uses almost no CPU

While no packet flows, the sender loop SHALL wait for `tun0` instead of reading it in a loop.

#### Scenario: Idle for two seconds
- **WHEN** the tunnel is up and idle for 2 seconds
- **THEN** the sender thread uses less than 20 clock ticks of CPU
