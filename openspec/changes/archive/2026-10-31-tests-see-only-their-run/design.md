## Context

`TunnelService.lastStatus` is process-wide and outlives a run. The instrumented tests run in one
process, one after another, and read it to see what a run did.

## Goals / Non-Goals

**Goals:**

- A test can only pass on a status or count produced by the run it started.
- Find the flaky test.

**Non-Goals:**

- Changing production code: all problems were in the tests.

## Decisions

### D1. Reset the status in `connect()`

The test helper sets `TunnelService.lastStatus` to "(test) not started" before starting the
service. A run always replaces it: with "Bad relay address", "No valid server key", "Waiting",
"Connected", an error or "Disconnected". So a condition on the status can only become true from
this run. Reset before `startService`, so the run cannot report before the reset.

### D2. Wait for the end of the run, not a fixed time

`runEnded()`: the status is no longer the sentinel, not Waiting or Connected, and the number of
live native sessions is back to the count at the start of the test. The session is freed before the
final status is reported, so both together mean the run is over. Shown to work in both directions:
with the error gate removed and a 6-second delay after the lookup, the test fails (the old one
passed); with the gate and the delay, it passes.

### D3. Leak check per test, 15 seconds

`@Before` records `Native.liveSessions()`; `@After` waits up to 15 seconds for the count to return
to it. With `Native.free` removed, each test fails with its own count (for example 10 for the
ten-round test, 2 for the two-address test), instead of every later test reporting the total. 15
seconds, because a stop cannot cut a name lookup short and the reviewer saw a lookup take 5 seconds.

### D4. The flaky test

Found while verifying this change: `anIdleTunnelUsesAlmostNoCpu` failed once with
`FileNotFoundException: /proc/self/task/3368/comm: ENOENT`. `senderCpuTicks()` lists the threads
and then reads each one's files; a thread that exits in between has no files. Reproduced with an
experiment that keeps starting and ending threads while reading: 137 of 3000 reads failed with the
same error; skipping a vanished thread, 0 of 3000. This is likely the failure the second review saw
without a test name, but that run left no name to confirm it.

## Risks / Trade-offs

- [Tests write a production field] → `lastStatus` is already public for the screen; the tests only
  reset it before starting a run.
