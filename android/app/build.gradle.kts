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

// Cross-compiles the Rust core for phones (arm64) and the emulator on Apple silicon or x86 hosts.
val buildRust by tasks.registering(Exec::class) {
    workingDir = repoRoot
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "x86_64",
        "-o", layout.buildDirectory.dir("rust/jniLibs").get().asFile.path,
        "build", "--release", "-p", "tunnel_core",
    )
}

// Only the public key goes into the APK; server.key in the same folder never does.
val copyServerKey by tasks.registering(Copy::class) {
    from(File(repoRoot, "keys")) { include("server.pub") }
    into(layout.buildDirectory.dir("generated/serverKey"))
    doLast {
        check(File(destinationDir, "server.pub").exists()) {
            "Run `cargo run -p relay -- keygen keys` in the repository root first."
        }
    }
}

tasks.named("preBuild") { dependsOn(buildRust, copyServerKey) }
