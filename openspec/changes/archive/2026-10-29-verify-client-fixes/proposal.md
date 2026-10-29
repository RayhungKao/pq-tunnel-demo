## Why

An independent review of `fix-client-lifecycle` reverted each fix in turn and ran the tests. Three
fixes had no test that failed when they were reverted, and two claims in that change were false:

- design.md D3 said closing `tun0` does not wake a thread blocked in `read()`. That was never
  reproduced. The tun fd is non-blocking, and the real bug was a busy loop: about 188 clock ticks of
  CPU in 2 idle seconds, nearly one core. `disconnectEndsAnIdleTunnelQuickly`, listed as covering
  the poll fix, passes with the poll removed.
- The README said the instrumented tests fail on a native crash. With `Native.free` moved before
  the receiver stops, all tests passed: the fake relay never sends anything, so nothing touches the
  freed handle.
- With the `Native.ready()` check removed, any stray datagram made the screen say "Connected", and
  no test noticed.

The review also found that a stop during a slow name lookup was followed by `establish()`, and that
`parseRelay` accepted text such as `10.0.2.2:51820:1` as an IPv6 address.

## What Changes

- Correct the stated reason for polling `tun0`, in code, design notes and README.
- Replace the raw pointer behind the JNI handle with a handle table. A call with a freed or unknown
  handle returns null or false; it cannot reach freed memory, whatever order Kotlin calls in.
- Look the relay name up before opening the socket or the interface, and check for a stop right
  after it. A failure after a stop is not shown as an error.
- `parseRelay` refuses a bare address with two or more colons unless it is hex digits and colons
  only, a bracketed address that is not IPv6, and ports with signs or other non-digits.
- New tests, each shown to fail with its fix reverted: idle CPU use, a stray datagram, handle table
  behavior, and parser inputs. A test that could not reproduce the slow-lookup bug was removed.
- Project rules in `CLAUDE.md` and `openspec/config.yaml`: a cause is written down only after it is
  reproduced, and a test is said to cover a fix only after it failed with the fix reverted.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `android-client`: native handle safety, idle CPU use, name lookup during a stop, stricter relay
  address parsing, the error text for an unresolvable host, and the stray-datagram case.

## Impact

- Code: `core/src/handles.rs` (new), `core/src/android.rs`, `core/src/lib.rs`, `Native.kt`,
  `TunnelService.kt`, `RelayAddress.kt`.
- Tests: 3 Rust tests, 2 JVM tests and 2 instrumented tests added; 1 instrumented test removed.
- The error for an unresolvable host now reads "Error: Unable to resolve host ..." instead of
  "Error: Unresolved address", because the lookup moved to `InetAddress.getByName`.
- Wire format unchanged.
