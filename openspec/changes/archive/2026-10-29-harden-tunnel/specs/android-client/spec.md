## ADDED Requirements

### Requirement: Only the tunnel subnet goes through the VPN

The app SHALL create a VPN interface with address 10.66.0.2/32, a route for 10.66.0.0/24 only and an
MTU of 1280, and SHALL exclude its own UDP socket from the VPN with `protect()`. Traffic outside
10.66.0.0/24 SHALL stay on the phone's normal network.

#### Scenario: Connect and ping the gateway
- **WHEN** the user taps Connect with relay `10.0.2.2:51820` on the emulator and allows the VPN
- **THEN** `tun0` has 10.66.0.2 and `ping 10.66.0.1` is answered

### Requirement: Bad input never crashes the app

The app SHALL accept a relay address as `host` or `host:port`, with port 51820 by default. An empty
host, a port that is not a number, or a port outside 1–65535 SHALL show "Bad relay address" and
start nothing. A missing or unreadable server key SHALL show "No valid server key in the APK".

#### Scenario: Port missing after the colon
- **WHEN** the user enters `10.0.2.2:` and taps Connect
- **THEN** the screen shows "Bad relay address: 10.0.2.2:" and the app keeps running

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
- **WHEN** the user taps Connect twice in a row
- **THEN** one connection runs, the screen shows "Connected", and pings are answered

#### Scenario: Disconnect
- **WHEN** the user taps Disconnect
- **THEN** `tun0` is removed and the screen shows "Disconnected"

### Requirement: Packet counts without flooding the UI thread

Each packet loop SHALL count only its own direction. The screen SHALL redraw the counts once a
second while it is visible and SHALL redraw the status when it changes.

#### Scenario: Counts while pinging
- **WHEN** three pings are answered
- **THEN** within a second the screen shows at least 3 sent and 3 received

### Requirement: A fresh clone builds a working APK or fails with a hint

The build SHALL fail before compiling when `keys/server.pub` is missing, with a message naming the
keygen command. It SHALL put only `server.pub` into the APK, never `server.key`. It SHALL find
`cargo` in `~/.cargo/bin` or `/opt/homebrew/opt/rustup/bin` when PATH has no Rust.

#### Scenario: No keys folder
- **WHEN** `./gradlew assembleDebug` runs in a clone without `keys/`
- **THEN** the build fails with "No keys/server.pub. Run `cargo run -p relay -- keygen keys`"

#### Scenario: Started without Rust on PATH
- **WHEN** the build runs with PATH set to `/usr/bin:/bin:/usr/sbin:/sbin`
- **THEN** the Rust core is still cross-compiled and the APK is built
