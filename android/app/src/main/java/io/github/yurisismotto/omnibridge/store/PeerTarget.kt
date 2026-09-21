package io.github.yurisismotto.omnibridge.store

/**
 * Which trusted computer this phone is trying to reach.
 *
 * ## The defect this exists to remove
 *
 * `ConnectionService` used to resolve its destination as
 * `trustStore.peers().firstOrNull()`, and `SendActivity` did the same. Storage
 * order was the routing table. Certified on real hardware (U2 §39.17): a
 * tablet trusting an Ubuntu desktop that was powered off and a Debian desktop
 * that was running dialled the Ubuntu entry forever and never sent the Debian
 * daemon a single packet — through an explicit Connect, an app restart and
 * eighty seconds of waiting. Deleting the Ubuntu *trust entry* — nothing else
 * changed, the Ubuntu machine never touched — and Debian connected in seven
 * seconds.
 *
 * ## The distinction that fixes it
 *
 * Trust and destination are different questions:
 *
 *  * **Trusted** — "may I authenticate this computer?" A property of the trust
 *    store, true for any number of peers at once, and *not* an instruction.
 *  * **Target** — "which computer am I connecting to right now?" At most one,
 *    chosen by a person, and anchored to a pinned fingerprint.
 *
 * Nothing here consults list position. [resolve] is a pure function of the
 * peer *set* and the chosen fingerprint, so reversing the trust store's order
 * cannot change its answer — the property `PeerTargetTest` asserts directly.
 *
 * ## When nobody has chosen
 *
 * The house rule already used for the clipboard tile and by the desktop CLI
 * for an ambiguous device prefix: **one candidate acts, zero or several ask.**
 * Picking for the person is how a file reaches the wrong computer, and there
 * is no version of that which is a good default. So with several trusted peers
 * and no choice made, this resolves to [MustChoose] and the UI says so, rather
 * than quietly dialling whichever record happens to be first.
 */
object PeerTarget {

    /** What the trust store plus the person's choice add up to. */
    sealed interface Resolution {

        /**
         * The one computer to act on, or null when there is no unambiguous one.
         *
         * A member rather than a `when` at each call site, so a future fifth
         * case cannot be silently forgotten by one of them.
         */
        fun peerOrNull(): TrustStore.TrustedPeer?

        /**
         * Why there is nothing to act on, in words a person can do something
         * about. Null when there *is* a target.
         *
         * Both non-null cases are terminal for the connection loop: no amount
         * of retrying invents a pairing or makes a choice, so the coordinator
         * stops and says which it is instead of backing off forever behind a
         * countdown nobody can satisfy.
         */
        fun blockedReason(): String?

        /** Every trusted computer that could be chosen, for a picker to show. */
        val candidates: List<TrustStore.TrustedPeer>

        /** The person chose this computer. Order in the trust store is irrelevant. */
        data class Selected(val peer: TrustStore.TrustedPeer) : Resolution {
            override fun peerOrNull() = peer
            override fun blockedReason(): String? = null
            override val candidates get() = listOf(peer)
        }

        /**
         * Nobody chose, but there is exactly one trusted computer, so there is
         * nothing to be ambiguous about.
         */
        data class OnlyTrustedPeer(val peer: TrustStore.TrustedPeer) : Resolution {
            override fun peerOrNull() = peer
            override fun blockedReason(): String? = null
            override val candidates get() = listOf(peer)
        }

        /** Nothing is paired. Pairing is the only thing that can help. */
        data object NoTrustedPeer : Resolution {
            override fun peerOrNull(): TrustStore.TrustedPeer? = null
            override fun blockedReason() = "no paired computer"
            override val candidates get() = emptyList<TrustStore.TrustedPeer>()
        }

        /**
         * Several trusted computers and none chosen.
         *
         * Deliberately carries no peer: this is exactly the case the old code
         * answered with `first()`.
         */
        data class MustChoose(override val candidates: List<TrustStore.TrustedPeer>) :
            Resolution {
            override fun peerOrNull(): TrustStore.TrustedPeer? = null
            override fun blockedReason() = "choose which computer to connect to"
        }
    }

    /**
     * Resolves the target from the trust store and the chosen fingerprint.
     *
     * [selectedHex] is a lowercase fingerprint hex, the same spelling
     * `Fingerprint.toHex` produces and the UI already uses for navigation
     * keys. A choice naming a computer that is no longer trusted is *ignored*,
     * not honoured and not an error: forgetting a computer must never leave a
     * dangling instruction to dial it, and must not fail closed either when
     * one other computer is left.
     */
    fun resolve(peers: List<TrustStore.TrustedPeer>, selectedHex: String?): Resolution {
        if (selectedHex != null) {
            // Matched by fingerprint, the pinned identity — never by name,
            // never by address, never by position.
            val chosen = peers.firstOrNull { it.fingerprint.toHex() == selectedHex }
            if (chosen != null) return Resolution.Selected(chosen)
        }
        return when (peers.size) {
            0 -> Resolution.NoTrustedPeer
            // `single`, not `[0]` or `first()`: the branch is already guarded
            // by the size, and spelling it this way leaves no positional peer
            // pick anywhere in the app for an audit to have to reason about.
            1 -> Resolution.OnlyTrustedPeer(peers.single())
            else -> Resolution.MustChoose(peers)
        }
    }

    /**
     * The same question for one capability: which computers could receive this?
     *
     * Used by the share sheet, where "eligible" is narrower than "trusted" —
     * a computer without the `files.v1` grant is not a destination even though
     * it is perfectly well paired. The resolution is then computed over the
     * *eligible* set, so a single eligible computer is used directly and
     * several make the person choose, exactly as [resolve] does.
     */
    fun resolveFor(
        peers: List<TrustStore.TrustedPeer>,
        selectedHex: String?,
        eligible: (TrustStore.TrustedPeer) -> Boolean,
    ): Resolution = resolve(peers.filter(eligible), selectedHex)

}
