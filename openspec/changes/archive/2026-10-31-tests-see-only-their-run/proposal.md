## Why

The third independent review found that two instrumented tests could pass with their bug present,
because `TunnelService.lastStatus` keeps the previous run's text and the tests read it:

- `anUnspecifiedAddressIsRefused` waited for any status starting with "Error:". After the `[::]`
  round the old error was still there, so the `0.0.0.0` round passed at once. Reproduced: with only
  `::` refused, the test passed.
- `aStopDuringTheNameLookupShowsDisconnectedNotTheLookupError` slept a fixed 5 seconds. The
  reviewer saw a lookup take 5048 ms; then the test reads the old "Disconnected". Reproduced: with
  the error gate removed and a 6-second delay after the lookup, the test passed.

It also found that README claimed a mutation check that no record backed, that a leak made every
later test fail, that the 5-second leak deadline was shorter than a slow lookup, and that the test
name described a stop "during" the lookup when the stop usually arrives before it starts.

While verifying the fixes, the suite failed once with
`anIdleTunnelUsesAlmostNoCpu | FileNotFoundException: /proc/self/task/3368/comm: ENOENT`.

## What Changes

- Every `connect()` in the tests first sets the status to "(test) not started", so a test sees only
  what its own run reports.
- `anUnspecifiedAddressIsRefused` waits for the exact message for each host.
- The lookup test waits until its run has ended (status changed and native session freed), up to
  15 seconds, instead of a fixed sleep, and is renamed
  `disconnectRightAfterConnectShowsDisconnectedNotTheLookupError`.
- The leak check compares with the count at the start of the test and allows 15 seconds, so only
  the leaking test fails.
- `senderCpuTicks()` skips a thread that exits between listing `/proc/self/task` and reading it.
- The handle-id mutation is recorded.
- A rule in `CLAUDE.md` and memory: a test waits for something only its own run can produce.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `android-client`: the leak check is per test; the stop scenario describes what happens.

## Impact

- Tests only: `android/app/src/androidTest/.../TunnelServiceTest.kt`. Production code is unchanged.
- Docs: README, CLAUDE.md.
