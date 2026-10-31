## Context

Same method as `verify-client-fixes`: every cause below was reproduced, and how is stated.

## Goals / Non-Goals

**Goals:**

- Each change has a test that failed with the change reverted, or an experiment that showed the
  difference, and the docs say which.
- Find the flaky test.

**Non-Goals:**

- Changing any timeout in the tests without knowing which one, if any, caused the flaky run.

## Decisions

### D1. One more stopped check, right before `establish()`

Reproduced with an experiment, since the window is a few milliseconds: with a 2-second sleep after
`sock.connect` and a stop 1 second after Connect, the log without the new check showed
`calling establish, stopped=true`; with the check, `establish()` was not called. A stop that lands
between this check and the end of `establish()` still brings `tun0` up briefly; the check after
`establish()` closes it. `establish()` itself cannot be interrupted, so the docs now say "closed at
once" for that case instead of "never".

### D2. Refuse the unspecified address after the lookup

Reproduced: `[::]:51820` showed "Waiting for the relay at :::51820" and never changed. The check is
`InetAddress.isAnyLocalAddress` after the lookup, not in `parseRelay`, so it also covers a host name
that resolves to `0.0.0.0`.

### D3. Count live native sessions

`Handles::len` and JNI `Native.liveSessions()`. Every instrumented test's `@After` waits up to 5
seconds for it to reach 0 and fails with the count otherwise. Shown to work: with `Native.free`
removed, `@After` failed.

### D4. The flaky run

Not reproduced. The original code ran the full instrumented suite 10 times with 0 failures, the
same code with the reviewer's mutation (no stopped check after the lookup) 10 times with 0
failures, and the original code 5 times with every host CPU busy (`yes` on all 8 cores) with 0
failures: 25 runs, 0 failures. No timeout was changed, since nothing points at one. If it happens
again, the test name and message are in
`android/app/build/outputs/androidTest-results/connected/debug/*.xml`; each `waitFor` failure prints
the status at the time.

## Risks / Trade-offs

- [`liveSessions()` is production code used only by tests] → One JNI function and one `len()`; it
  cannot change behavior.
- [The stopped check before `establish()` narrows the window but cannot close it] → Documented;
  the existing check after `establish()` closes `tun0` at once.
