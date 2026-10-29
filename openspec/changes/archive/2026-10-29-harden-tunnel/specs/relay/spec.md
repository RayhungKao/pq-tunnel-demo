## ADDED Requirements

### Requirement: Key generation never overwrites a pinned key

`relay keygen <dir>` SHALL write the server's private keys to `<dir>/server.key` with mode `0600`
and the base64 public keys to `<dir>/server.pub`. It SHALL refuse to overwrite an existing
`server.key`, because clients built with the old public key would stop working.

#### Scenario: Second keygen into the same folder
- **WHEN** `relay keygen keys` runs while `keys/server.key` exists
- **THEN** it exits with an error and both files are unchanged

### Requirement: One session per INIT, answered with the same ACK

When an INIT is accepted, the relay SHALL open a session under its session id and send the ACK to
the sender. When an INIT maps to a session id it already has, the relay SHALL send that session's
original ACK again and SHALL keep the session, including its replay window, unchanged.

#### Scenario: Ping through the tunnel
- **WHEN** a client completes the handshake over UDP and sends a sealed ICMP echo request from
  10.66.0.2 to 10.66.0.1
- **THEN** it receives the sealed echo reply

#### Scenario: A replayed INIT does not reopen recorded packets
- **WHEN** an attacker resends a recorded INIT and then a recorded DATA packet the relay already
  answered
- **THEN** the relay sends the original ACK, which the client ignores
- **AND** the relay does not answer the recorded packet again
- **AND** the client's next new packet is still answered

#### Scenario: The tunnel recovers after a relay restart
- **WHEN** a relay restarts with the same long-term key and the client resends its INIT
- **THEN** a DATA packet recorded before the restart gets no answer
- **AND** after the new ACK, the client's pings are answered again

### Requirement: Replies follow the client's latest address

The relay SHALL send each reply to the source address of the packet it answers, so a session keeps
working when the client's address changes.

#### Scenario: The client changes networks
- **WHEN** authentic DATA for an existing session arrives from a new address
- **THEN** the reply goes to the new address

### Requirement: Packets from the tunnel cannot crash the relay

The relay SHALL answer only ICMP echo requests to the gateway 10.66.0.1, with source and destination
swapped, TTL 64, type 0 and valid IPv4 and ICMP checksums. It SHALL treat any packet whose IPv4
header length is below 20 bytes, or which is shorter than its header plus 8 bytes, as not a ping,
without reading past its end.

#### Scenario: A valid ping gets a valid reply
- **WHEN** the relay builds the reply to an echo request from 10.66.0.2 to 10.66.0.1
- **THEN** the addresses are swapped, the type is 0, and both checksums verify

#### Scenario: Pings to other addresses are not answered
- **WHEN** the echo request is addressed to 10.66.0.9, or the packet is a single byte
- **THEN** there is no reply

#### Scenario: A short header length does not panic
- **WHEN** a session sends `40 00 00 00 00 00 00 00` (IPv4, header length 0) or a 12-byte packet with
  header length 4
- **THEN** there is no reply and the relay keeps serving
