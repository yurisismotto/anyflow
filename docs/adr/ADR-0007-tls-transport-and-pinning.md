# ADR-0007 — TLS 1.3 transport and public-key pinning

**Status:** Accepted · 2026-08-29

## Context

Every byte between the devices must be encrypted and authenticated (principles
5 and 8). There is no CA and there never will be one: provisioning a PKI for
two devices on a home network is absurd. So the usual chain validation has
nothing to validate against, and we have to supply our own notion of identity.

## Decision

**TLS 1.3 only, mutual authentication, SPKI pinning.**

* Both sides present their identity certificate. The desktop requires client
  authentication (`client_auth_mandatory() -> true`) — a peer with no
  certificate has no identity and can never be authorized.
* The client pins the server's SPKI fingerprint, taken from the QR code at
  pairing time and from the trust store afterwards.
* The server accepts a structurally valid unknown client certificate, extracts
  its SPKI, and defers the **authorization** decision to the session layer,
  which requires either a trust-store hit or a valid pairing proof.
* Only TLS 1.3 is compiled in. Both `verify_tls12_signature` implementations
  return an error unconditionally.
* ALPN `anyflow/1` on both ends.
* TLS 1.3 session tickets are disabled (`send_tls13_tickets = 0`): resumption
  would let a peer skip a full handshake, and the full handshake is where the
  pinning check lives.
* Certificates carry **no** subject alternative names, and the client verifier
  ignores the server name entirely.

`rustls` with the *ring* provider on Rust; `SSLContext.getInstance("TLSv1.3")`
with a custom `X509TrustManager` and `X509ExtendedKeyManager` on Android.

## Alternatives

**WebPKI with Let's Encrypt.** Rejected: requires public DNS names and
internet reachability for a link-local service. Absurd here.

**A local CA, with the desktop as the root.** Would let the desktop issue
certificates to phones and use normal chain validation. Rejected: it adds CA
key management, revocation lists and expiry handling to solve a problem that
direct key pinning solves without any of it. Two devices do not need a PKI.

**Noise Protocol Framework (as WireGuard uses).** Genuinely attractive — it is
designed for exactly this shape of problem, with static public keys as
identity and no certificates at all. **Rejected for v1** because Android's TLS
stack is what can use a non-exportable Keystore key: a Noise implementation
would need raw key access, so the phone's key would have to be exportable
software key material. That trade is not worth the elegance. If Android ever
exposes ECDH with a Keystore key in a way Noise can use, this is worth
revisiting.

**TLS raw public keys (RFC 7250).** Exactly what we want conceptually — the
certificate here is only a container for a public key. Rejected: support is
thin, particularly on Android. A self-signed certificate is the portable way
to carry a raw public key through TLS.

**Trust on first use, KDE Connect style.** Rejected: it leaves a MITM window
on the first connection. The QR code carries the fingerprint out of band
precisely so there is no such window.

**Certificate pinning instead of SPKI pinning.** Rejected — see ADR-0006.

## Consequences

* Two custom certificate verifiers exist, and they are the most
  security-critical code in the project. Both files carry a prominent warning.
* Hostname verification is off, deliberately. Certificates carry no SANs so
  nobody can reintroduce name-based trust by accident.
* Certificates expire after ten years. Because the SPKI is pinned, they can be
  reissued from the same key without breaking pairings — but the reissue path
  is not implemented yet, and is recorded as a debt. See
  [Certificate renewal](#certificate-renewal) for the intended strategy.
* No resumption means every reconnection is a full handshake. At the frequency
  connections actually happen, the cost is irrelevant.

## Certificate renewal

Reviewed and deliberately left unimplemented for the foundation Sprint. There
is no bug that requires rotation now, and building it would be speculative
work against an event ten years away. What matters today is that the design
*permits* it, and that the strategy is written down rather than rediscovered
under time pressure.

**The identity is the key, not the certificate.** A peer is identified by
`SHA-256(DER SubjectPublicKeyInfo)`. The certificate is a disposable container
that exists only because TLS requires one; it carries no SANs, no CA chain and
no authority. Both the trust store and the QR payload record the fingerprint,
never the certificate.

So renewal is:

1. Keep the existing private key. On Android this is not merely preferred but
   forced — the key is non-exportable inside the Keystore, and re-issuing is
   the *only* thing that can be done to it.
2. Issue a fresh self-signed certificate over that same key, with the same
   subject and a new validity window.
3. Replace the stored certificate. Every pinned fingerprint on every paired
   device still matches, because the SPKI did not change. **No re-pairing.**

The expiry check in both verifiers (`leaf.checkValidity()` on Android, rustls'
own validity handling on the desktop) is hygiene, not the trust anchor: it
exists so a forgotten certificate surfaces as a clear error instead of working
forever. That is exactly why it is safe for renewal to be a local, unilateral
operation needing no coordination between peers.

**What would break this, and must not be done:** pinning the certificate
instead of the SPKI, putting anything identity-bearing in the certificate that
peers rely on, or generating a new key pair during renewal. The last one is
not renewal at all — it is a new identity, and it correctly requires
re-pairing.

**When to implement.** Before the first release with a supported upgrade path,
or whenever certificate generation stops being a fresh-install-only event.
The trigger to watch is a device whose certificate is within a year of
`not_after`; a daemon that finds one should reissue at startup. The shared
test fixtures in `protocol/testdata/` expire in 2036 and will need
regenerating well before then, which serves as an early reminder.

## Security implications

The single most dangerous mistake available in this design is a verifier that
returns success from `verify_tls13_signature` (Rust) or has an empty
`checkServerTrusted` (Android). Either one silently removes all security while
looking like working code, because **certificates are public** — anyone can
replay a copy of a legitimate one. What proves the peer *holds* the private
key is the handshake signature.

Therefore:

* `PinnedServerCertVerifier::verify_tls13_signature` delegates to
  `rustls::crypto::verify_tls13_signature`. It does not shortcut.
* `RecordingClientCertVerifier::verify_tls13_signature` does the same. This is
  what makes it safe for `verify_client_cert` to accept an unknown
  certificate: the SPKI extracted afterwards provably belongs to whoever is on
  the socket.
* Android's `PinnedTrustManager.checkServerTrusted` compares the SPKI and
  throws on mismatch; the platform verifies the handshake signature.

There is **no** build flag, debug variant or test helper anywhere in this
repository that relaxes any of the above.

Tested by `a_different_server_identity_is_rejected_by_the_client`, which
asserts the exact rustls cause (`ApplicationVerificationFailure`) rather than
merely that something failed, and by
`a_client_without_a_certificate_is_rejected_during_the_handshake`.
