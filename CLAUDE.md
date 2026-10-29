# pq-tunnel-demo

Android VPN demo: Rust `core/` (tunnel crypto + JNI), Rust `relay/`, Kotlin `android/`. Behavior is
specified in `openspec/specs/`; changes go through `openspec/changes/<name>/` and are archived.

## How every change is made
1. Before changing anything, run the full suite and note the result (the baseline).
2. Reproduce the problem. A cause goes into a comment, doc or spec only after it has been
   reproduced (a log, a measurement, a small experiment); until then it is a hypothesis.
3. Write the test first and watch it fail.
4. Make the change; run the full suite again.
5. Revert the change and confirm the new test fails; restore it. Only then may a doc say the test
   guards the change. If a bug cannot be tested deterministically, make it impossible by design.
6. After restoring a mutation or experiment, rebuild before any check by hand: Gradle test tasks
   also rebuild the debug APK, so the APK on disk may still contain the mutation.
7. Review the diff line by line. Anything not checked is written as "not verified" with the reason.

## Commands
Environment: `export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:/opt/homebrew/opt/openjdk@17/bin:$PATH" JAVA_HOME=/opt/homebrew/opt/openjdk@17 ANDROID_HOME=$HOME/Library/Android/sdk`
- Rust: `cargo test`
- Android JVM: `cd android && ./gradlew testDebugUnitTest`
- Android device (emulator `Pixel_3a_API_32_arm64-v8a` running): `cd android && ./gradlew connectedDebugAndroidTest`
- Specs: `openspec validate --all --strict`
