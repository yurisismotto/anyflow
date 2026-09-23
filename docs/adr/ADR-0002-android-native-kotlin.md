# ADR-0002 — Native Kotlin on Android

**Status:** Accepted · 2026-08-29

## Context

The Android app must hold a private key in hardware, run a foreground service
under Android's modern background-execution rules, open a TLS 1.3 socket with
a custom trust manager, and browse mDNS. All four are platform APIs with no
meaningful cross-platform abstraction.

## Decision

Native Kotlin, Jetpack Compose for UI, coroutines for concurrency, targeting
API 35 with a minimum of API 29.

API 29 (Android 10) is the floor because it is where TLS 1.3 is enabled by
default and where `SSLParameters.setApplicationProtocols` (ALPN) became
available. Below it we could not speak the protocol at all.

## Alternatives

**Flutter or React Native.** Would eventually help if an iOS client happened.
Rejected: every security-critical API here (Keystore, `X509TrustManager`,
`NsdManager`, foreground service types) would need a platform channel, so we
would write the Kotlin anyway *and* add a large dependency surface between the
user and their keys.

**Kotlin Multiplatform, sharing protocol code with the desktop.** Attractive
on paper. Rejected: the desktop is Rust, so KMP would mean a third language
choice or rewriting the daemon in Kotlin/JVM — which would cost the memory
safety and the small, dependency-light daemon that motivated ADR-0003.

**Rust on Android via JNI, sharing `omnibridge-core`.** Genuinely tempting: one
implementation of the protocol, no drift. Rejected for this Sprint because the
private key must live in the Android Keystore and be used for TLS client
authentication, which means the TLS stack must be Conscrypt, which means the
handshake lives on the Java side regardless. Splitting the protocol across a
JNI boundary mid-handshake buys complexity, not safety. Worth revisiting once
the protocol stabilises and the shared part is pure logic.

## Consequences

* Two implementations of the protocol to keep in sync. Mitigated by shared
  `.proto` files (ADR-0001) and by a cross-language known-answer test on the
  pairing MAC.
* Full access to platform APIs with no bridge layer.
* Compose keeps the UI small; there is very little of it in this Sprint by
  design.

## Security implications

* Positive: the private key is generated inside the Android Keystore and never
  leaves it. No abstraction layer can accidentally copy it into managed
  memory.
* Positive: fewer third-party dependencies in the app. The only non-AndroidX
  runtime dependencies are protobuf-javalite and ZXing.
* Negative: two implementations mean two places to get the pairing MAC wrong.
  This is the specific risk the cross-language test exists to catch.
