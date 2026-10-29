## ADDED Requirements

### Requirement: The client reports when it is ready

The client SHALL report that it is ready exactly when it has accepted an ACK and can seal DATA, so a
caller can tell a relay that answered from one that did not.

#### Scenario: Not ready before the ACK
- **WHEN** the client has sent INIT and no ACK has arrived
- **THEN** it reports not ready

#### Scenario: Ready after a valid ACK
- **WHEN** the client accepts the server's ACK
- **THEN** it reports ready

#### Scenario: Still not ready after a refused ACK
- **WHEN** the client refuses an ACK from a server with different keys
- **THEN** it reports not ready
