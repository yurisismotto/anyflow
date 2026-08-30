# ADR-0005 — LAN discovery via mDNS/DNS-SD

**Status:** Accepted · 2026-08-29

## Context

The phone must find the desktop on a home or office network where addresses
are handed out by DHCP and change. No cloud rendezvous service is permitted
(principle 2), so discovery has to be link-local.

## Decision

DNS-SD over mDNS, service type `_fedroid-bridge._tcp.local.`

**The desktop advertises; the phone browses and always initiates the
connection.** The direction is fixed.

TXT record:

```
v  = 1        TXT schema version
pv = "1-1"    supported protocol version range
id = <hex>    device id
dn = <name>   device name
```

The identity fingerprint is **not** published.

Rust uses `mdns-sd` (pure Rust, no Avahi dependency); Android uses the
platform `NsdManager` with a `MulticastLock` held while browsing.

**Discovery is not trust.** Everything in a record is attacker-controlled and
is used only to decide what is worth dialling.

## Alternatives

**UDP broadcast on a fixed port**, as KDE Connect does. Simple and avoids an
mDNS stack. Rejected: broadcast is increasingly filtered on modern Wi-Fi
(client isolation, IGMP snooping), it needs a fixed port with no clean
fallback, and it reimplements service discovery badly.

**Bluetooth LE presence.** On the roadmap as a complement — it works when the
devices are on different networks, and it is a better proximity signal.
Rejected as the primary mechanism: BLE on Android requires location-adjacent
permissions the app does not otherwise need, and throughput is unsuitable for
the data channel.

**Manual IP entry only.** Rejected as the only option — an address that
changes every lease is a bad user experience — but kept as a fallback: the QR
code carries address hints, and the phone remembers the last working address.

**Phone advertises, desktop connects.** Rejected: a phone's address changes
constantly, its ability to accept inbound connections is unreliable, and it
sleeps. Fixing the direction also means there is exactly one handshake path
to reason about.

**Publishing the fingerprint in TXT** so the phone can recognise the right
service without connecting. Rejected: it would let a passive observer
enumerate the trust graph on any network the machine joins.

## Consequences

* Works out of the box on typical home networks.
* Fails on networks with client isolation or across VLANs. The remembered
  address and the QR hints cover part of that; a manual-address UI is a debt.
* The desktop must keep an mDNS responder running. `--no-mdns` disables it,
  and a failure to publish is logged but is not fatal — pairing by QR carries
  explicit addresses.

## Security implications

* **Discovery grants nothing.** A spoofed record leads to a TCP connection
  that fails the pinned-key check. This invariant is stated in the module docs
  on both sides and is tested by `an_unpaired_device_cannot_use_the_protocol`.
* Discovery data is sanitized before display: device names are stripped of
  control characters and capped at 64 characters, and a device id that is not
  hex is discarded.
* **Privacy cost, accepted:** publishing a stable `id` and `dn` on an
  untrusted network is a tracking signal. Documented in the threat model
  (T18). Not publishing the fingerprint limits the damage to "this machine was
  here", not "this machine trusts that phone". A per-network advertising
  toggle is the proper fix and is on the roadmap.
