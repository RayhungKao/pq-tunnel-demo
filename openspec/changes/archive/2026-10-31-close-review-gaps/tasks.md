Baseline before any change, at 3552d1e: Rust 20, JVM 7, instrumented 7, all passing. Instrumented
tests ran on the `Pixel_3a_API_32_arm64-v8a` emulator.

## 1. Flaky run (design D4)

- [x] 1.1 Original code, full instrumented suite, 10 runs: 0 failures
- [x] 1.2 Reviewer's mutation (no stopped check after the lookup), 10 runs: 0 failures
- [x] 1.3 Original code with all 8 host cores busy, 5 runs: 0 failures
- [x] 1.4 Not reproduced; no timeout changed

## 2. Changes and how each was checked

| Change | Check | Without the change | With the change |
|---|---|---|---|
| Error for the unspecified address (D2) | `anUnspecifiedAddressIsRefused` | fails: "Waiting for the relay at :::51820" | passes |
| No error shown after a stop (gate from verify-client-fixes) | `aStopDuringTheNameLookupShowsDisconnectedNotTheLookupError` | fails 3 of 3 runs: the lookup error is shown | passes |
| Every session freed (D3) | `@After` of every instrumented test | fails when `Native.free` is removed | passes |
| `Handles::len` | `len_counts_live_handles` | not mutation-checked; it pins the count | passes |
| Stopped check before `establish()` (D1) | experiment, no automated test | `calling establish, stopped=true` logged | `establish()` not called |

- [x] 2.1 Found in review: the two lookup tests used the fixed name `relay.invalid`. Measured on
  the emulator, a failed lookup takes 124 ms for a new name and 0 ms when repeated (Android caches
  failures), so a cached failure could beat the stop. Both tests now use a fresh
  `relay-<nanoTime>.invalid`; the gate mutation was re-run after this change and failed 3 of 3
- [x] 2.2 `android/.kotlin/` (Kotlin compiler session files) added to `.gitignore`

## 3. Full suite after the changes

- [x] 3.1 Rust 21 (core 14, relay 7), JVM 7, instrumented 9; the instrumented suite passed 3 runs
  out of 3
- [x] 3.2 By hand on an APK built from the final production code (the later edits touched only tests
  and docs), with the relay: `10.0.2.2:` refused,
  `relay.invalid` shows the lookup error, `[::]` shows "Error: :: is not a relay address", relay
  off shows Waiting, relay on shows Connected within 11 seconds, 3 of 3 pings answered with 4 sent
  and 3 received, relay restart heals, Connect twice works, Disconnect removes `tun0` and leaves no
  `tunnel` thread, no `FATAL EXCEPTION`

## 4. Not verified

- The stopped check after the name lookup (from verify-client-fixes) still has no automated test;
  it was shown by the experiment recorded there.
- What caused the one flaky run in the review.
- A failed send during a network switch, a real slow resolver, a physical phone.
