Method for every row: run the full suite first (baseline: Rust 17, JVM 5, instrumented 5, all
passing at 4d8d823), write or keep the test, run it against the code with the fix reverted, then
against the fixed code. Instrumented tests ran on the `Pixel_3a_API_32_arm64-v8a` emulator.

## 1. Rules

- [x] 1.1 `CLAUDE.md`: how every change is made, including rebuilding after a mutation before any
  check by hand
- [x] 1.2 `openspec/config.yaml`: design states only reproduced causes; tasks name how each test was
  shown to fail

## 2. Fixes and the tests that cover them

| Fix | Test | With the fix reverted | With the fix |
|---|---|---|---|
| Poll `tun0` (design D1) | `anIdleTunnelUsesAlmostNoCpu` | fails: 188 ticks in 2 s | passes: 0 ticks |
| `Native.ready()` gate | `aStrayDatagramIsNotTakenForTheAck` | fails: "Connected" after 40 stray bytes | passes |
| Handle table (design D2) | `a_freed_handle_finds_nothing` | fails when `remove` keeps the entry | passes |
| Handle table | `ids_are_never_zero_and_never_reused`, `a_call_in_progress_keeps_the_object_alive` | not mutation-checked; they pin the stated behavior | pass |
| Stricter `parseRelay` (design D4) | `extraColonsAndOddPortsAreRefused` | fails on `10.0.2.2:51820:1` with the old parser | passes |
| Bracketed IPv6 | `anIpv6RelayAddressIsUsed` | fails when `[` input is refused | passes |
| `Run.stop()` closes the run | `disconnectEndsAnIdleTunnelQuickly` | fails when `stop()` is empty | passes |
| Lookup before opening (design D3) | none | experiment: `establish, stopped=true` logged | experiment: no `establish` logged |

- [x] 2.1 Poll `tun0`, with the corrected reason in the code comment
- [x] 2.2 Handle table in `core/src/handles.rs`; JNI and `Native.kt` return null or false for a
  missing handle
- [x] 2.3 Name lookup before the socket, stopped check after it, no error after a stop. **Correction:** the
  "no error after a stop" gate had no test at the time and was not marked "not verified";
  close-review-gaps adds `aStopDuringTheNameLookupShowsDisconnectedNotTheLookupError`
- [x] 2.4 Stricter `parseRelay`
- [x] 2.5 Delete `disconnectDuringNameLookupEndsDisconnected`, which passed with the bug present

Checked earlier by the independent review of 4d8d823 and still valid for unchanged code:
`withoutAnAckItSendsInitAndKeepsWaiting` fails when "Connected" is shown right after establish;
`anUnresolvableHostShowsTheErrorAndKeepsIt` fails when the error is overwritten;
`repeatedConnectAndDisconnectEndCleanly` fails with `stopSelf()` and without the stopped checks.

## 3. Full suite after the changes

- [x] 3.1 Rust 20 (core 13, relay 7), JVM 7, instrumented 7, all passing
- [x] 3.2 By hand on a freshly built APK with the relay: unresolvable host shows "Error: Unable to
  resolve host ...", relay off shows Waiting, relay on shows Connected within 11 seconds, 3 of 3
  pings answered and the counts show 4 sent and 3 received, relay restart heals, Connect twice
  works, Disconnect removes `tun0` and leaves no `tunnel` thread, no `FATAL EXCEPTION`

## 4. Not verified

- A failed send during a network switch: the emulator does not make `send` fail.
- The slow-lookup fix on a real slow resolver: only the injected sleep was tested.
- A physical phone.
