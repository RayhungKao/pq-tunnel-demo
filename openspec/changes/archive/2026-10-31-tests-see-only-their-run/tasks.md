Baseline before any change, at 363c188: Rust 21, JVM 7, instrumented 9, all passing. Production
code is unchanged by this change. Instrumented tests ran on the `Pixel_3a_API_32_arm64-v8a`
emulator.

## 1. Reproduce the review's findings first

- [x] 1.1 Only `::` refused (0.0.0.0 accepted): the old `anUnspecifiedAddressIsRefused` passed
- [x] 1.2 Error gate removed plus a 6-second delay after the lookup: the old lookup test passed

## 2. Changes and how each was checked

| Change | Mutation or experiment | Result with the new test |
|---|---|---|
| Exact message per host (D1) | only `::` refused | fails: still "Waiting for the relay at 0.0.0.0:51820" |
| Wait for the end of the run (D2) | error gate removed | fails |
| Wait for the end of the run (D2) | error gate removed, lookup 6 s slower | fails (the old test passed) |
| Wait for the end of the run (D2) | gate present, lookup 6 s slower | passes: no false failure |
| Per-test leak check (D3) | `Native.free` removed | each test fails with its own count, e.g. "10 native sessions from this test were never freed" |
| Skip vanished threads (D4) | experiment, threads starting and ending while reading | 137 of 3000 reads failed before, 0 of 3000 after |
| `senderCpuTicks()` still sees a spin | poll removed | `anIdleTunnelUsesAlmostNoCpu` fails: 180 ticks in 2 s |
| Handle ids not reused | ids taken from `live.len() + 1` | `ids_are_never_zero_and_never_reused` fails |

- [x] 2.1 Reset the status in `connect()`
- [x] 2.2 Exact message per host in `anUnspecifiedAddressIsRefused`
- [x] 2.3 `runEnded()` and the renamed lookup test
- [x] 2.4 Leak check against the count at the start of the test, 15 seconds
- [x] 2.5 `senderCpuTicks()` skips vanished threads
- [x] 2.6 After archiving, rename the scenario "Disconnect during a lookup that then fails" in
  `openspec/specs/android-client/spec.md` to "Disconnect right after Connect to a name that cannot
  be resolved": OpenSpec does not let a MODIFIED block drop or rename a scenario, and in the
  third review's timing logs the stop usually arrived before the lookup started
- [x] 2.7 Rule in CLAUDE.md and memory: wait for something only the test's own run can produce

## 3. Full suite

- [x] 3.1 Rust 21, JVM 7, instrumented 9. Instrumented: 3 runs before the ENOENT fix (one failed
  with ENOENT, which led to D4), then 10 runs after it with 0 failures
- [x] 3.2 No check by hand: production code is identical to 363c188, which was checked by hand

## 4. Not verified

- That the second review's unnamed failure was the ENOENT race (it left no test name).
- A physical phone, a real slow resolver, a failed send during a network switch.
