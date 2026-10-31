## MODIFIED Requirements

### Requirement: Every native session is freed

When a connection ends, for any reason, its native session SHALL be freed, so the number of live
sessions returns to what it was before the connection started.

#### Scenario: After each instrumented test
- **WHEN** a test has stopped its connection and the status is no longer Waiting or Connected
- **THEN** within 15 seconds the number of live native sessions is no higher than at the start of
  that test

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
- **WHEN** the user connects to a name under `.invalid` and disconnects at once (in the third
  review's timing logs the stop usually arrived before the lookup started)
- **THEN** once the run has ended, the screen shows "Disconnected", not the lookup error, however
  long the lookup took
