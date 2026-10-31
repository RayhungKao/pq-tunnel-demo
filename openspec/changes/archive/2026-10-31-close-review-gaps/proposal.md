## Why

The independent review of `verify-client-fixes` confirmed every mutation check that change claimed,
and found four gaps and one flaky run:

- README said a stop that arrives early "never brings up `tun0`". A stop that lands after the last
  check before `establish()` still let it run; `tun0` came up and was closed at once.
- design.md set the goal that every fix has a test or is impossible by construction, but the
  stopped check after the name lookup and the "no error after a stop" gate had neither.
- `[::]` and `0.0.0.0` were accepted as a relay, and the app waited for an answer forever.
- Removing `Native.free` left every test passing: nothing counted live native sessions.
- One full instrumented run failed one test under a mutation that should have changed nothing; the
  output did not say which test.

## What Changes

- A stopped check right before `establish()`.
- An error for a relay name that resolves to the unspecified address.
- `Native.liveSessions()` (backed by `Handles::len`) and a check in every instrumented test's
  `@After` that it returns to 0.
- A test that a stop during a failing lookup ends with "Disconnected".
- Docs: the stop wording, the design goal, and the flaky-run investigation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `android-client`: unspecified relay addresses, freeing every native session, a stop near
  `establish()`, and the precise wording of "Stopping always works".

## Impact

- Code: `TunnelService.kt`, `Native.kt`, `core/src/android.rs`, `core/src/handles.rs`.
- Tests: 1 Rust test and 2 instrumented tests added; every instrumented test now also checks for a
  leaked native session.
- Wire format unchanged.
