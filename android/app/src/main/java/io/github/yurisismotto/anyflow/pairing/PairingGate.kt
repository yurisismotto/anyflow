package io.github.yurisismotto.anyflow.pairing

/**
 * Keeps the reconnect loop out of the way while a scanned code is being
 * proved.
 *
 * ## UX-DEBT-02
 *
 * Two paths on this phone can open a socket to the same computer, and only
 * one of them carries a pairing token:
 *
 *  * `AnyFlowApp.pair` — a scan. Dials with the QR's token and will answer a
 *    `PAIR_REQUEST` challenge;
 *  * `AnyFlowApp.connect`, driven by `ConnectionService`'s coordinator — a
 *    reconnection. Dials with **no** token, by design: a reconnection is not
 *    a pairing and must never carry pairing authority.
 *
 * With the desktop having revoked this phone and a fresh pairing window open,
 * the tokenless path reaches a desktop that is *waiting for a pairing proof*.
 * `PeerConnection.handshake` then stops at
 *
 * ```kotlin
 * val token = pairingToken ?: return ConnectResult.PairingRequired
 * ```
 *
 * having sent no `PAIR_REQUEST`, and the desktop sits on that connection
 * until it times out. Reproduced on the certification hardware, with no scan
 * involved at all:
 *
 * ```text
 * daemon:  a revoked device is pairing again; it must prove the new token
 *          and be confirmed by hand  peer=573C CB84 DA6C 993B
 * daemon:  connection ended ... error=protocol violation: timed out waiting
 *          for PAIR_REQUEST
 * logcat:  CONNECT_FAILURE round=1 kind=terminal
 *          reason=the computer no longer knows this device
 * logcat:  STOPPED reason=the computer no longer knows this device
 * ```
 *
 * Which is the recorded defect, verbatim. The reconnect loop then declares
 * the peer terminal and stops, while the person is in the middle of trying to
 * re-pair it.
 *
 * ## The rule
 *
 * A scan is an explicit instruction about *that* computer. For as long as it
 * is being proved, the tokenless path holds off — for that computer and no
 * other, because a pairing with one desktop is no reason to drop the link to
 * a different one.
 *
 * The hold is **transient**, never terminal: the coordinator backs off and
 * tries again, so a pairing that fails leaves the link exactly where it was
 * rather than stopping the loop. Nothing here weakens revocation — a revoked
 * device still gets `HELLO_STATUS_REJECTED` and still stops — and nothing
 * here grants trust. It only decides which of this phone's two dialers may
 * hold the socket.
 */
object PairingGate {

    /** Said to the user and written to the log. Carries no token. */
    const val REASON = "a pairing code is being proved for this computer"

    /**
     * Why a reconnect dial must hold off, or null to go ahead.
     *
     * [pairingHex] is the computer a scan is currently proving a token for,
     * or null when no scan is in flight. Compared case-insensitively because
     * a fingerprint hex reaches this from two places — a trust store record
     * and a QR payload — and matching is the whole job.
     */
    fun holdFor(targetHex: String, pairingHex: String?): String? =
        if (pairingHex != null && pairingHex.equals(targetHex, ignoreCase = true)) {
            REASON
        } else {
            null
        }
}
