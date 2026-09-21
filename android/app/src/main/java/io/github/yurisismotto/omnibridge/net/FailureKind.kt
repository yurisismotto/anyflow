package io.github.yurisismotto.omnibridge.net

import java.security.cert.CertificateException
import javax.net.ssl.SSLPeerUnverifiedException

/**
 * Why a connection attempt failed, at the only granularity that changes what
 * we do about it.
 *
 * Reconnection must treat "the Wi-Fi dropped" and "that is not the computer I
 * paired with" as different events. Retrying the first quickly is correct;
 * retrying the second quickly achieves nothing, because a peer that fails the
 * pinning check will not start passing it, and dialling it in a tight loop is
 * the closest thing this design has to an online oracle.
 */
enum class FailureKind {
    /** The network, the route, the socket. Retried on the normal ladder. */
    TRANSPORT,

    /**
     * Authentication: the pinned key did not match, or the peer could not
     * prove it holds it. Retried slowly, and never by relaxing the pin.
     */
    SECURITY,

    /** The computer no longer trusts this device. Only a human can fix it. */
    REVOKED,
}

/**
 * Decides which kind a thrown exception represents.
 *
 * Conscrypt reports a pinning failure as an `SSLHandshakeException` whose
 * cause chain contains the `CertificateException` that
 * [PinnedTrustManager] threw, so the chain has to be walked rather than the
 * top-level type inspected.
 *
 * The default is [FailureKind.TRANSPORT], deliberately. Misreading a security
 * failure as transport costs some pointless retries against a peer that will
 * keep being rejected — the pin holds either way. Misreading a transport
 * failure as security would leave a phone on a fifteen-minute ladder because
 * the router rebooted, which is a far worse outcome.
 */
object FailureClassifier {

    /** Bounded so a self-referential cause chain cannot spin. */
    private const val MAX_CAUSE_DEPTH = 16

    fun classify(error: Throwable?): FailureKind {
        var current = error
        var depth = 0
        while (current != null && depth < MAX_CAUSE_DEPTH) {
            if (current is CertificateException || current is SSLPeerUnverifiedException) {
                return FailureKind.SECURITY
            }
            val next = current.cause
            if (next === current) break
            current = next
            depth += 1
        }
        return FailureKind.TRANSPORT
    }
}
