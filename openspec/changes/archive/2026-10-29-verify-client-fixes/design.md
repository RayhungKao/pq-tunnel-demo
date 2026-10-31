## Context

Every cause in this document was reproduced on the `Pixel_3a_API_32_arm64-v8a` emulator, and the
way it was reproduced is stated next to it. Where something was not reproduced, this says so.

## Goals / Non-Goals

**Goals:**

- Every fix in `android/` and `core/` has a test that fails when the fix is reverted, or the bug is
  made impossible by construction.

> **Correction (close-review-gaps):** this goal was not met. The stopped check after the name
> lookup and the "no error after a stop" gate had neither a test nor a design that made the bug
> impossible; close-review-gaps adds a test for the gate and records the check as shown by experiment only.

- Docs state only causes that were reproduced.

**Non-Goals:**

- Full IPv6 address validation, and IPv6 zone ids such as `fe80::1%wlan0`.
- A deterministic test for a slow name lookup: there is no way to slow DNS from the test without a
  hook in production code.

## Decisions

### D1. Why `tun0` is polled (corrects fix-client-lifecycle D3)

Reproduced: `/proc/<pid>/fdinfo/<tun fd>` of the running app shows `flags: 02404002`, which
includes `O_NONBLOCK` (04000). A loop that calls `read()` directly returns 0 at once when no packet
is waiting and goes around again. Measured with `anIdleTunnelUsesAlmostNoCpu`: with the poll removed
the `tunnel` thread used 188 ticks in 2 seconds (the clock is 100 ticks per second), with the poll
0. `Os.poll` with a 500 ms timeout waits without using CPU and still notices a stop within half a
second.

### D2. A handle table instead of a raw pointer

Kotlin held a `Long` that was a pointer to a Rust `Box`. A call after `free` read freed memory, and
only the order of calls in `TunnelService.finally` prevented it. A test for that ordering would be
probabilistic: a use-after-free crashes only sometimes. `core/src/handles.rs` keeps sessions in a
map from id to `Arc`; `free` removes the entry, a lookup of a missing id returns nothing, and a call
that already holds the `Arc` keeps the session alive until it returns. The JNI functions return
null or false for a missing handle. Ids start at 1 and are never reused, because Kotlin treats 0 as
"no session". This module builds and is tested on every platform, not only Android.

### D3. Look the name up first

Reproduced with an experiment, since a test cannot slow DNS: with `Thread.sleep(1500)` before
`sock.connect` and a log line before `establish()`, a connect followed at once by a stop logged
`establish, stopped=true`. The lookup used to happen inside `sock.connect`, after the stopped check.
It now happens in `InetAddress.getByName(host)`, followed by a stopped check, before the socket or
the interface is opened. The same experiment with the sleep after the lookup logged no
`establish`. An error that happens after a stop is not reported, because the stop itself is the
reason the connection ended.

### D4. Relay address parsing

A bare address with two or more colons is accepted only when it is hex digits and colons. Dots are
allowed only inside brackets, for an embedded IPv4 address (`[::ffff:10.0.2.2]`). A port must be one
to five plain digits. This is a rough check, not full RFC 4291 validation; the name lookup rejects
anything that passes it but is not an address.

### D5. How each test was checked

Each test was run once against the code with its fix reverted and once against the fixed code. The
table in tasks.md lists the mutation and the result. A test that passed with its bug present
(`disconnectDuringNameLookupEndsDisconnected`) was deleted rather than kept as false comfort.

## Risks / Trade-offs

- [The CPU test measures time] → Its bound is 20 ticks in 2 seconds, against 0 measured with the
  fix and 188 without; a loaded machine has a wide margin.
- [The slow-lookup fix has no automated test] → It was verified by the experiment in D3, which was
  removed afterwards; tasks.md marks it as verified by hand.
