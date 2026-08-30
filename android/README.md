# AnyFlow — Android app

Kotlin, Jetpack Compose, coroutines. No Google Play Services, no analytics, no
network access beyond the LAN socket to the paired computer.

## Build

Requires **JDK 21** and the **Android SDK, platform 35** with build-tools
35.0.0. Point Gradle at them with `JAVA_HOME` and `ANDROID_HOME`, or with an
untracked `android/local.properties`.

JDK 21 specifically: AGP 8.8 does not support running on JDK 25, which is what
Fedora 44 ships as its default `java`.

```bash
export JAVA_HOME=/path/to/jdk-21
export ANDROID_HOME="$HOME/Android/Sdk"

cd android
./gradlew :app:testDebugUnitTest   # 63 tests
./gradlew :app:assembleDebug       # -> app/build/outputs/apk/debug/app-debug.apk
```

### Resource limits

`gradle.properties` caps the Gradle JVM, the Kotlin daemon and the worker
count on purpose. This is a single-module project, so parallelism buys almost
nothing, while the defaults are enough to push a 16 GiB laptop into swap. Do
not raise them without a reason.

## Testing

Local JVM unit tests cover the wire rules, pinning, framing, capability
negotiation and the pairing proof. Two suites assert known-answer vectors
shared with the Rust implementation:

* `PairingProofTest` — the proof and confirmation HMACs.
* `FingerprintTest` — the SPKI fingerprints of `protocol/testdata/*.der`,
  which are real certificates emitted by the desktop identity code.

`PinnedTrustManagerTest` runs against those same real certificates rather than
a stub, so removing the pinning comparison makes it fail.

**Not covered here:** `TrustStore`'s persistence path needs a real `Context`
and `filesDir`, and proof-of-private-key-possession is the TLS handshake's
job, which a local unit test cannot stand in for. Both are on-device
concerns.

## Layout

| Path | Role |
| --- | --- |
| `identity/DeviceIdentity.kt` | P-256 key in the Android Keystore, StrongBox when available |
| `identity/Fingerprint.kt` | SHA-256 over the DER SPKI |
| `net/PinnedTrustManager.kt` | TLS 1.3 + public-key pinning. **Read the comments before editing.** |
| `net/Framing.kt` | Length-prefixed protobuf frames |
| `net/PeerConnection.kt` | Handshake, pairing, replay guard, capability routing |
| `net/Discovery.kt` | mDNS/DNS-SD browsing via `NsdManager` |
| `pairing/QrPayload.kt` | Strict parser for the scanned code |
| `pairing/PairingProof.kt` | HMAC-SHA256 proof, identical to the Rust side |
| `capability/` | The plugin model, plus `battery.v1` |
| `service/ConnectionService.kt` | `connectedDevice` foreground service |
| `store/TrustStore.kt` | Paired computers, on disk |

## Permissions, and what is deliberately missing

Held: `INTERNET`, `ACCESS_NETWORK_STATE`, `CHANGE_WIFI_MULTICAST_STATE`,
`CHANGE_NETWORK_STATE`, `FOREGROUND_SERVICE`,
`FOREGROUND_SERVICE_CONNECTED_DEVICE`, `POST_NOTIFICATIONS`, `CAMERA`.

Not held, and not to be added without an ADR: any accessibility service, the
notification listener, `QUERY_ALL_PACKAGES`, location, or broad storage. The
project does not require root and does not use ADB.

## Protobuf

The Gradle protobuf plugin compiles `../../protocol/proto` directly — the same
files the Rust daemon compiles. There is no second copy to drift.
