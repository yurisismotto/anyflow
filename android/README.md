# Fedroid Bridge — Android app

Kotlin, Jetpack Compose, coroutines. No Google Play Services, no analytics, no
network access beyond the LAN socket to the paired computer.

## Build

```bash
cd android
./gradlew :app:assembleDebug
```

> **Not yet built.** The environment this Sprint was developed in had no JDK
> and no Android SDK, so this module has **never been compiled**. The Kotlin
> is written against documented APIs and mirrors the Rust implementation
> function for function, but expect to fix version pins in
> `gradle/libs.versions.toml` and possibly a few import or API details on the
> first real build. This is recorded as a known limitation, not as a claim
> that it works. See the Sprint report.

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
