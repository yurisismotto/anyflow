# ADR-0017 — Runtime-narrowable capability roles

**Status:** Accepted · 2026-09-08

The first time a capability negotiates anything of its own. This is recorded as
an ADR because it is a **pattern**, not a feature: the reasoning will apply
again to the next capability whose two ends are not symmetric.

## Context

`notifications.v1` is asymmetric in two independent ways.

**Across platforms.** Android can observe its own notifications and cannot
usefully display a desktop's; Linux can display and, in v1, cannot observe;
Windows can do both; iOS can barely do either. A peer must never infer that
because it implements one half, its peer implements the other.

**Across time.** Android notification access is a permission the user grants in
Settings and can withdraw at any moment. Withdrawal fires
`onListenerDisconnected()` while the AnyFlow session is up and healthy. The
phone must be able to say **"I am no longer a source"** immediately, without a
reconnect — and *without* dropping `notifications.v1` entirely, because it
should still accept dismissals for notifications it has already sourced.

### Why `HELLO` cannot express this

`HELLO.capabilities` is a flat list of ids, and the transport is deliberately
capability-agnostic — [ADR-0008](ADR-0008-capability-architecture.md):
*"There is no capability name anywhere in `anyflow-core`'s transport code."*
Three properties of `HELLO` each rule it out on their own:

1. **It is sent once.** A handshake field cannot express a permission that
   changes at 14:32 on a connection established at 09:00.
2. **It is a list of ids.** Adding role vocabulary would push notification
   semantics into the handshake, which is what makes the transport
   feature-blind in the first place.
3. **It is all-or-nothing per capability.** Dropping `notifications.v1` from a
   re-advertised set is the only narrowing `HELLO` can express, and it is too
   coarse: it would also stop the phone accepting dismissals it can still
   honour.

### The alternative that was rejected

**Four capability ids** — `notifications.v1/source`, `/sink`,
`/dismiss-target`, `/dismiss-reporter`. This is exactly the shape ADR-0008
rejected for `clipboard.v1`, and it fails here for the additional reason that a
capability set only narrows at a reconnect. A revoked permission would leave a
peer advertising `notifications.v1/source` until the session ended.

## Decision

**Roles are declared inside the capability, carried in its own message, and are
narrowable and widenable at runtime.**

### 1. The vocabulary

```protobuf
enum NotificationRole {
  NOTIFICATION_ROLE_UNSPECIFIED      = 0;
  NOTIFICATION_ROLE_SOURCE           = 1;  // I can mirror my notifications to you
  NOTIFICATION_ROLE_SINK             = 2;  // I can display yours
  NOTIFICATION_ROLE_DISMISS_TARGET   = 3;  // I will act on your DismissRequest
  NOTIFICATION_ROLE_DISMISS_REPORTER = 4;  // I will tell you when a human closed a mirror
}

message NotificationRoles {
  repeated NotificationRole roles = 1;   // the complete set, not a delta
  uint32 epoch = 2;                      // monotonic per connection, from 1
}
```

In v1: Android advertises `SOURCE` and `DISMISS_TARGET`; Linux advertises
`SINK` and `DISMISS_REPORTER`.

### 2. Absent roles mean no roles — fail closed

A peer that never sends a `NotificationRoles` message gets nothing sent to it
and has nothing accepted from it. This is the default, and it is what lets a
minimal, older or partially-implemented peer interoperate harmlessly rather
than by accident.

An **empty** announcement means the same thing as never announcing, on purpose.
It is exactly what an Android device sends the instant notification access is
revoked, and the two states must be indistinguishable.

### 3. Narrowing takes effect immediately, and so does widening

A role announcement is applied on receipt. Unlike a capability grant — which
needs a reconnect to *widen*, because widening is the dangerous direction for
an authorization — a role is a statement about what the peer can physically do
right now, and delaying it in either direction is wrong:

* delaying a **narrowing** means content continues to flow to a device that
  will drop it, or worse, to one whose user just revoked the permission;
* delaying a **widening** means a user who just granted access sees nothing
  until they reconnect, and concludes the feature is broken.

Neither direction is a privilege change, which is why neither needs the
reconnect discipline a grant needs (§6).

### 4. The epoch, and what it is for

`epoch` is monotonic per connection, starting at 1. A receiver applies an
announcement only if its epoch is **strictly greater** than the last accepted.
Epoch 0 means "unset" and is refused.

This exists for exactly one failure: a reordered, duplicated or replayed
announcement re-widening a set that has already narrowed. Concretely — the
phone announces `{SOURCE}`, the user revokes access, the phone announces `{}`,
and a duplicate of the first message arrives afterwards. Without the epoch, the
desktop resumes accepting notifications from a device whose user just said no.

Strictness matters: *equal* is refused too, so a duplicate of the current epoch
carrying a different set cannot take effect either.

The epoch is **per connection**, not persisted. A new session starts at 1, and
role state is not carried across a reconnect — there is nothing to
resynchronise, because each side announces first thing on connect.

### 5. Unknown roles are ignored, never assumed

A future role value is dropped, and the rest of the announcement still applies.
One unknown value must not discard a set, and it must not be inferred into
existence: a peer that has never heard of a role cannot have implemented it, so
treating an unknown value as granted could only ever be wrong.

### 6. **Platform capability ≠ AnyFlow peer grant**

This is the clause the rest of the ADR exists to protect, and it is the easiest
one to erode by accident.

|  | Platform capability | AnyFlow peer grant | Role |
| --- | --- | --- | --- |
| Question it answers | *May this app read notifications on this phone?* | *May this specific pinned computer be sent them?* | *Can this peer physically do this half of the capability right now?* |
| Who decides | The OS, in Settings | The user, per peer, in AnyFlow | The peer, about itself |
| Where it lives | Android permission state | The local trust store | One connection's memory |
| Is it authorization? | For the OS | **Yes** | **No** |

**A role is never an authorization input.** It is a peer's claim about itself,
and its only job is to stop a sender wasting content on a device that would drop
it. Three consequences, all normative:

* A peer **cannot widen its own grants** by claiming a role. There is no code
  path from a received `NotificationRoles` message to the trust store, and
  there must never be one — the same "a peer can never set its own policy" rule
  `clipboard.v1` enforces by absence.
* **Holding the OS permission grants nothing to any peer.** Notification access
  alone sends nothing to anyone ([ADR-0015 §1 clause 4](ADR-0015-notification-access.md)).
* **Holding a peer grant does not manufacture a platform capability.** A
  granted peer that has no OS permission announces no `SOURCE` role and mirrors
  nothing. The grant is not a promise the device can keep.

Collapsing any two of these three into one switch would mean a person who
wanted a phone-side capability had silently authorised a network destination.

### 7. Role mismatch is not a session error

Sending an upsert to a peer that never claimed `SINK` is a bug on the sender's
side. The receiver answers `REJECTED_ROLE` and **does not close the session** —
one capability misbehaving must not cost the user everything else
([ADR-0008](ADR-0008-capability-architecture.md)).

## Consequences

* **The revocation story is honest.** "AnyFlow stops mirroring the moment you
  revoke access in Settings" is structurally true: the phone announces the
  narrowed set before it stops being connected, and the desktop closes every
  mirror for that peer.
* **Each capability owns its own vocabulary.** The transport still knows no
  feature names, and `HELLO` is unchanged. `battery.v1`, `files.v1` and
  `clipboard.v1` are untouched.
* **The pattern generalises.** Any future capability whose two ends are not
  symmetric, or whose local permission can be withdrawn mid-session, gets this
  shape rather than a new handshake field. A Windows or macOS adapter declares
  its roles and needs no protocol change.
* **Each side keeps one small piece of per-connection state** — a role set and
  an epoch — dropped when the connection ends.
* **`epoch` is a `uint32`.** Exhausting it needs four billion permission
  changes on one connection; the session would have to survive them all, and a
  wrap would refuse further updates rather than accept a stale one, which is
  the safe failure.

### What this costs

A capability now has state that must be announced before it can be used, which
is one more thing an implementer can forget. The fail-closed default is what
makes forgetting safe: an implementation that never announces is inert rather
than wrong.

## Notes

* Wire schema: `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto`;
  rules in [02 §3](../research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md).
* Portable implementation: `anyflow_core::notifications::PeerRoles`.
* The access contract, and the permission lifecycle this narrows against:
  [ADR-0015](ADR-0015-notification-access.md).
* Notification naming: [ADR-0016](ADR-0016-notification-identity.md).
* The capability model this extends: [ADR-0008](ADR-0008-capability-architecture.md).
