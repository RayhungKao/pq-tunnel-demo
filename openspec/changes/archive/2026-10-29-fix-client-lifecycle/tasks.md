Rust tests are named after the scenario they cover. JVM tests are in `android/app/src/test/`,
instrumented tests in `android/app/src/androidTest/` (run with `./gradlew connectedDebugAndroidTest`
on the `Pixel_3a_API_32_arm64-v8a` emulator). Checks marked "by hand" used `adb` and the relay.

## 1. Status (android-client, tunnel-protocol)

- [x] 1.1 `Client::is_ready()` and JNI `Native.ready()`; asserted in `nothing_is_sent_before_the_ack`,
  the `handshake` test helper and `ack_from_the_wrong_server_is_rejected`
- [x] 1.2 "Waiting for the relay" until the first ACK, then "Connected" (design D1); instrumented
  `withoutAnAckItSendsInitAndKeepsWaiting`; by hand: relay off shows Waiting, starting the relay
  shows Connected within 11 seconds
- [x] 1.3 Keep the error instead of "Disconnected" (design D2); instrumented
  `anUnresolvableHostShowsTheErrorAndKeepsIt`

## 2. Relay address

- [x] 2.1 `parseRelay` as a pure function with IPv6 forms; JVM `RelayAddressTest` (5 tests,
  including ten inputs that must be refused)
- [x] 2.2 Instrumented `anIpv6RelayAddressIsUsed` sends INIT to `[::1]:<port>`

## 3. Stopping (found by the new tests)

- [x] 3.1 Poll `tun0` with a 500 ms timeout (design D3). **Correction:** the stated cause is wrong and the listed test passes
  without the poll; see verify-client-fixes. Found when the cleanup of
  `withoutAnAckItSendsInitAndKeepsWaiting` timed out in "Waiting"; `disconnectEndsAnIdleTunnelQuickly`
  was added after the fix to cover it and has not been run against the old code
- [x] 3.2 `Run` object per connection (design D4); instrumented
  `repeatedConnectAndDisconnectEndCleanly` hung in "Waiting" before the fix
- [x] 3.3 `stopSelf(startId)` (design D5); the same test ended in "Disconnected" instead of
  "Waiting" before the fix
- [x] 3.4 Run the instrumented suite three times in a row: 5 of 5 pass each time

## 4. Build

- [x] 4.1 Key check in `buildRust.doFirst`; by hand: a copy without `keys/` fails in 1 second with
  the hint and no Rust crate compiled

## 5. Verify by hand on the emulator

- [x] 5.1 Relay off, Connect: "Waiting for the relay at 10.0.2.2:51820"
- [x] 5.2 Start the relay: "Connected" within 11 seconds
- [x] 5.3 `ping -c 3 10.66.0.1`: 3 of 3 answered; 1.5 seconds later the screen shows 4 sent,
  3 received (the extra one is a packet Android sends on the interface)
- [x] 5.4 Disconnect: "Disconnected" within 1.5 seconds, `tun0` removed, no `FATAL EXCEPTION`

## 6. Docs

- [x] 6.1 README: status flow, test commands and counts, verification table
