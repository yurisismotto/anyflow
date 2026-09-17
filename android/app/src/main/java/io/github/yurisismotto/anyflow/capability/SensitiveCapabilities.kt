package io.github.yurisismotto.anyflow.capability

/**
 * The capabilities a pairing never grants on its own.
 *
 * ## Why this is one named set rather than two subtractions
 *
 * Re-pairing an already-known computer **merges** the fresh record into the
 * existing one, so that a person's local decisions survive a desktop-side
 * revoke ([io.github.yurisismotto.anyflow.store.TrustStore.TrustedPeer.mergePairing]).
 * Grants are unioned, which is what preserves a grant the person turned on.
 *
 * A union is also how a grant could be *invented*. Until this set existed, the
 * only thing stopping that was a subtraction written inline in `AnyFlowApp`
 * when the fresh record was built:
 *
 * ```kotlin
 * grantedCapabilities = connection.negotiatedCapabilities
 *     .toSet() - ClipboardCapability.ID - NotificationsCapability.ID
 * ```
 *
 * That held, and it held for the wrong reason: the merge knew nothing about
 * sensitivity, so the non-escalation property was an emergent consequence of a
 * line in a different file. Any change to that line — a refactor, a widened
 * auto-grant policy, a third sensitive capability added without remembering to
 * subtract it — would have let a re-pair silently hand a computer the
 * notification stream because the *negotiation* advertised it.
 *
 * So the rule is named once, here, and enforced where the union happens.
 *
 * ## What "sensitive" means
 *
 * A capability whose grant reveals something the pairing itself is not consent
 * for, and which therefore needs its own, separately revocable yes from the
 * device card:
 *
 *  * `clipboard.v1` — a computer that can write this phone's clipboard can
 *    also see what is pasted next;
 *  * `notifications.v1` — a computer that can see this phone's notifications
 *    sees banking alerts, 2FA codes and message previews, and on the
 *    certification hardware the platform's own OTP redaction did not fire at
 *    all. It is never in `auto_grant` (ADR-0015 §4).
 *
 * `files.v1` and `battery.v1` are deliberately **not** here. Pairing may grant
 * them: a file still needs per-transfer approval, and a battery percentage is
 * not a secret. See the comment at the pairing site for why each is where it
 * is.
 *
 * Adding a capability to this project is the moment to ask which side of this
 * set it belongs on. Adding one to *this set* is a widening of a user
 * protection and nothing here should ever be removed without an ADR.
 */
object SensitiveCapabilities {

    /**
     * Never granted by pairing — only from the device card, by hand.
     *
     * Read by the two places that can put a capability into a trust record
     * during pairing: the construction of the fresh record, and the merge.
     */
    val NEVER_AUTO_GRANTED: Set<String> = setOf(
        ClipboardCapability.ID,
        NotificationsCapability.ID,
    )

    /** Whether granting [capabilityId] needs its own explicit yes. */
    fun isSensitive(capabilityId: String): Boolean =
        capabilityId in NEVER_AUTO_GRANTED
}
