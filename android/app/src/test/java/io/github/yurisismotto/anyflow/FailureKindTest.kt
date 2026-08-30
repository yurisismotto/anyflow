package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.net.FailureClassifier
import io.github.yurisismotto.anyflow.net.FailureKind
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.security.cert.CertificateException
import javax.net.ssl.SSLException
import javax.net.ssl.SSLHandshakeException
import javax.net.ssl.SSLPeerUnverifiedException
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Telling "the network had a bad day" apart from "that is not my computer".
 *
 * These two must not share a retry policy: the first should be retried
 * promptly, and the second should not be hammered — a peer failing the
 * pinning check will keep failing it, and dialling it in a tight loop is the
 * closest thing this design has to an online oracle.
 */
class FailureKindTest {

    @Test
    fun `a pinning failure is a security failure even when TLS wraps it`() {
        // The real shape on Android: Conscrypt reports our own
        // CertificateException as the cause of an SSLHandshakeException.
        val wrapped = SSLHandshakeException("handshake failed").apply {
            initCause(CertificateException("server identity mismatch: expected AAAA, got BBBB"))
        }
        assertEquals(FailureKind.SECURITY, FailureClassifier.classify(wrapped))
    }

    @Test
    fun `a bare certificate exception is a security failure`() {
        assertEquals(
            FailureKind.SECURITY,
            FailureClassifier.classify(CertificateException("mismatch")),
        )
    }

    @Test
    fun `an unverified peer is a security failure`() {
        assertEquals(
            FailureKind.SECURITY,
            FailureClassifier.classify(SSLPeerUnverifiedException("no certificate")),
        )
    }

    @Test
    fun `a security failure is found several causes deep`() {
        val deep = IOException(
            "outer",
            SSLException("middle", CertificateException("inner")),
        )
        assertEquals(FailureKind.SECURITY, FailureClassifier.classify(deep))
    }

    @Test
    fun `ordinary network errors are transport failures`() {
        assertEquals(
            FailureKind.TRANSPORT,
            FailureClassifier.classify(ConnectException("Connection refused")),
        )
        assertEquals(
            FailureKind.TRANSPORT,
            FailureClassifier.classify(SocketTimeoutException("timeout")),
        )
        // A TLS-layer error is not automatically a pinning failure: a reset
        // mid-handshake is the network, and treating it as an identity
        // problem would put a phone on a fifteen-minute ladder because the
        // router rebooted.
        assertEquals(
            FailureKind.TRANSPORT,
            FailureClassifier.classify(SSLException("Connection reset by peer")),
        )
    }

    @Test
    fun `nothing at all is a transport failure`() {
        assertEquals(FailureKind.TRANSPORT, FailureClassifier.classify(null))
    }

    @Test
    fun `a self-referential cause chain terminates`() {
        val looping = object : IOException("loop") {
            override val cause: Throwable get() = this
        }
        assertEquals(FailureKind.TRANSPORT, FailureClassifier.classify(looping))
    }
}
