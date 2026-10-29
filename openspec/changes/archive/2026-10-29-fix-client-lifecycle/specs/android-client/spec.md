## ADDED Requirements

### Requirement: The status says whether the relay answered

After the VPN interface is up, the app SHALL show "Waiting for the relay at <host>:<port>" until the
relay's ACK arrives, and "Connected to <host>:<port> (X25519 + ML-KEM-768)" from then on. When a
connection ends with an error, the app SHALL keep showing that error instead of "Disconnected".

#### Scenario: Relay not running
- **WHEN** the user taps Connect and nothing answers at the relay address
- **THEN** the screen shows "Waiting for the relay" and never "Connected"

#### Scenario: Relay starts later
- **WHEN** the relay starts while the app is waiting
- **THEN** within about 10 seconds, at the next INIT resend, the screen shows "Connected"

#### Scenario: Unresolvable host
- **WHEN** the user connects to `relay.invalid:51820`
- **THEN** the screen shows "Error: Unresolved address" and keeps showing it

## MODIFIED Requirements

### Requirement: Bad input never crashes the app

The app SHALL accept a relay address as `host`, `host:port`, `[ipv6]`, `[ipv6]:port` or a bare IPv6
address, with port 51820 by default. An empty host, a colon without a port after it, a port that is
not a number, or a port outside 1–65535 SHALL show "Bad relay address" and start nothing. A missing
or unreadable server key SHALL show "No valid server key in the APK".

#### Scenario: Port missing after the colon
- **WHEN** the user enters `10.0.2.2:` and taps Connect
- **THEN** the screen shows "Bad relay address: 10.0.2.2:" and the app keeps running

#### Scenario: IPv6 relay
- **WHEN** the relay address is `[::1]:<port>`
- **THEN** the INIT is sent to that IPv6 address and port

### Requirement: One connection at a time, cleaned up by its owner

Starting a connection SHALL first stop the previous one. A stop SHALL end the connection whatever
state it is in, including before it has opened its socket or interface and while no packet is
flowing, within about one second. Each connection SHALL close only the socket and interface it
created and SHALL free its native session only after both packet loops have ended. A Disconnect
SHALL not stop a Connect that was requested after it. Status messages from a connection that a newer
one replaced SHALL not be shown.

#### Scenario: Connect tapped twice
- **WHEN** the user taps Connect twice in a row with the relay running
- **THEN** one connection runs, the screen shows "Connected", and pings are answered

#### Scenario: Disconnect
- **WHEN** the user taps Disconnect
- **THEN** `tun0` is removed and the screen shows "Disconnected"

#### Scenario: Disconnect on an idle tunnel
- **WHEN** the tunnel has carried no packets for 3 seconds and the user disconnects
- **THEN** the screen shows "Disconnected" within 2 seconds

#### Scenario: Connect and Disconnect in quick succession
- **WHEN** Connect and Disconnect are sent ten times in a row, half of the time with no pause
  between a Connect and the next request
- **THEN** every Disconnect ends with "Disconnected", every Connect reaches "Waiting for the relay",
  and the process does not crash

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
