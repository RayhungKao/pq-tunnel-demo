## ADDED Requirements

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

## MODIFIED Requirements

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
