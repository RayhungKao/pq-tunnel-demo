## ADDED Requirements

### Requirement: An unspecified relay address is refused

When the relay name resolves to the unspecified address (`::` or `0.0.0.0`), the app SHALL show an
error instead of waiting for an answer that cannot come.

#### Scenario: Unspecified IPv6 and IPv4 addresses
- **WHEN** the user connects to `[::]:51820` or `0.0.0.0:51820`
- **THEN** the screen shows "Error: <host> is not a relay address"

### Requirement: Every native session is freed

When a connection ends, for any reason, its native session SHALL be freed, so the number of live
sessions returns to 0 once no connection runs.

#### Scenario: After each instrumented test
- **WHEN** a test has stopped its connection and the status is no longer Waiting or Connected
- **THEN** the number of live native sessions returns to 0 within 5 seconds

### Requirement: A stop near establish()

A stop that arrives before the last check before `establish()` SHALL not bring up `tun0`. A stop
that arrives while `establish()` runs cannot interrupt it; `tun0` SHALL be closed as soon as
`establish()` returns.

#### Scenario: Stop while the socket is being connected
- **WHEN** the user disconnects after the socket was opened and before `establish()` is called
- **THEN** `establish()` is not called and the screen ends with "Disconnected"

## MODIFIED Requirements

### Requirement: Stopping always works

A stop SHALL end the connection whatever state it is in. Once the socket exists it SHALL end within
about one second; during a slow name lookup the run SHALL end as soon as the lookup returns, without
opening anything. An error caused by the stop SHALL not be shown. A Disconnect SHALL not stop a
Connect that was requested after it.

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
- **THEN** neither the socket nor `tun0` is opened, and the screen ends with "Disconnected"

#### Scenario: Disconnect during a lookup that then fails
- **WHEN** the user connects to `relay.invalid:51820` and disconnects at once
- **THEN** the screen ends with "Disconnected", not with the lookup error
