Each Rust test is named after the scenario it covers. Android scenarios have no automated test; they
were checked by hand on the `Pixel_3a_API_32_arm64-v8a` emulator with `adb` (see section 6).

## 1. Setup

- [x] 1.1 Run `openspec init --tools claude` in the repository root
- [x] 1.2 Run `openspec validate harden-tunnel --strict` and fix what it reports

## 2. Handshake with a server random (tunnel-protocol)

- [x] 2.1 ACK packet with a fresh random and a truncated HMAC-SHA256 tag (design D1, D2);
  tests `nothing_is_sent_before_the_ack`, `ack_from_the_wrong_server_is_rejected`,
  `forged_ack_is_rejected`
- [x] 2.2 Traffic keys from the random; the client switches on a new ACK, ignores the current one and
  refuses used ones (design D4); test `restarted_server_derives_new_keys`
- [x] 2.3 Keep the existing DATA and replay-window behavior; tests `both_directions_round_trip`,
  `keys_survive_save_and_load`, `tampering_is_rejected`,
  `replays_are_rejected_but_reordering_is_fine`, `packets_older_than_the_window_are_rejected`,
  `malformed_init_is_rejected`
- [x] 2.4 JNI: `seal` returns null before the ACK, `open` handles ACKs and returns null for them

## 3. Relay

- [x] 3.1 Check the IPv4 header length before reading fields; test
  `short_header_length_does_not_panic`, plus `reply_has_swapped_addresses_and_valid_checksums` and
  `only_pings_to_the_gateway_get_replies`
- [x] 3.2 Keep each session's ACK and resend it for a repeated INIT (design D3); tests
  `ping_through_the_tunnel_end_to_end`, `replaying_init_does_not_reopen_old_packets`,
  `tunnel_recovers_after_a_relay_restart`
- [x] 3.3 Test for replies to the latest address: `replies_follow_the_clients_latest_address`
- [x] 3.4 Check by hand that a second `keygen` into the same folder exits 1 and leaves both files
  unchanged, and that `server.key` has mode 600

## 4. Android client

- [x] 4.1 Validate the relay address and read the server key inside the error handling
- [x] 4.2 Resend INIT every 10 seconds on a daemon thread that never touches the native handle
  (design D5)
- [x] 4.3 Catch send failures per packet; drop packets sealed before the ACK. Not reproduced on the
  emulator: a network switch there does not make `send` fail
- [x] 4.4 Close only the run's own socket and interface; drop status from replaced runs (design D6)
- [x] 4.5 Count each direction on its own thread; redraw the screen once a second

## 5. Build

- [x] 5.1 Move the `server.pub` check from `copyServerKey.doLast` to `preBuild.doFirst` (design D7)
- [x] 5.2 Find `cargo` in the rustup homes, pass PATH and `ANDROID_HOME` to `cargo ndk` (design D8)
- [x] 5.3 Check by hand: a copy without `keys/`, built with PATH `/usr/bin:/bin:/usr/sbin:/sbin`,
  compiles the Rust core and then fails with the keygen hint
- [x] 5.4 Check by hand: the real repository builds with the same minimal PATH

## 6. Verify on the emulator

- [x] 6.1 `10.0.2.2:` shows "Bad relay address" and the app process keeps its pid
- [x] 6.2 Connect: `tun0` has 10.66.0.2, three pings answered
- [x] 6.3 Restart the relay: pings answered again within about 10 seconds
- [x] 6.4 Tap Connect twice: "Connected", three pings answered
- [x] 6.5 Disconnect: `tun0` removed, "Disconnected", no `FATAL EXCEPTION` in logcat

## 7. Docs

- [x] 7.1 Update README: protocol, known limits, run steps, tests, link to `openspec/specs/`
