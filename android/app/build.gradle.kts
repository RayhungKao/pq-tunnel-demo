plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "io.github.rayhungkao.pqtunnel"
    compileSdk = 34

    defaultConfig {
        applicationId = "io.github.rayhungkao.pqtunnel"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rust/jniLibs"))
    sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/serverKey"))
}

dependencies {
    implementation("androidx.activity:activity-ktx:1.9.3")
}

val repoRoot = rootProject.projectDir.parentFile

// Android Studio started from the Dock gets the system PATH, which has no Rust. Look in the usual
// rustup homes (the official installer, then Homebrew's keg-only rustup) before falling back to PATH.
val cargoDirs = listOf("${System.getProperty("user.home")}/.cargo/bin", "/opt/homebrew/opt/rustup/bin")
val cargo = cargoDirs.map { File(it, "cargo") }.firstOrNull { it.canExecute() }?.path ?: "cargo"

// Cross-compiles the Rust core for phones (arm64) and the emulator on Apple silicon or x86 hosts.
val buildRust by tasks.registering(Exec::class) {
    workingDir = repoRoot
    // cargo needs these dirs on PATH to find the cargo-ndk subcommand, and cargo-ndk needs the SDK
    // to find the NDK.
    environment("PATH", (cargoDirs + System.getenv("PATH").orEmpty()).joinToString(File.pathSeparator))
    environment("ANDROID_HOME", android.sdkDirectory.path)
    commandLine(
        cargo, "ndk", "-t", "arm64-v8a", "-t", "x86_64",
        "-o", layout.buildDirectory.dir("rust/jniLibs").get().asFile.path,
        "build", "--release", "-p", "tunnel_core",
    )
}

// Only the public key goes into the APK; server.key in the same folder never does.
val copyServerKey by tasks.registering(Copy::class) {
    from(File(repoRoot, "keys")) { include("server.pub") }
    into(layout.buildDirectory.dir("generated/serverKey"))
}

tasks.named("preBuild") {
    dependsOn(buildRust, copyServerKey)
    // Checked here, not in copyServerKey: Gradle skips a Copy task with no source files
    // (NO-SOURCE), actions included, so a fresh clone without keys/ would build an APK with no key.
    doFirst {
        check(File(repoRoot, "keys/server.pub").exists()) {
            "No keys/server.pub. Run `cargo run -p relay -- keygen keys` in the repository root first."
        }
    }
}
