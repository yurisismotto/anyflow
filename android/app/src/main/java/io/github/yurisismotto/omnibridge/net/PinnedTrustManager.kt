package io.github.yurisismotto.omnibridge.net

import io.github.yurisismotto.omnibridge.identity.DeviceIdentity
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import java.net.Socket
import java.security.Principal
import java.security.PrivateKey
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509TrustManager

/**
 * TLS 1.3 with public-key pinning.
 *
 * ## Read this before changing anything here
 *
 * This is the file where a small mistake silently removes all security. The
 * rules:
 *
 *  * [PinnedTrustManager.checkServerTrusted] must compare the presented key
 *    against the pinned one and throw otherwise. An empty method body is the
 *    classic "trust all certificates" hole, and it looks identical to working
 *    code until someone attacks you.
 *  * The peer proving it *holds* the private key is done by the TLS stack
 *    itself as part of the handshake signature. Pinning alone would be
 *    worthless, because certificates are public and anyone can replay a copy.
 *  * Only TLSv1.3 is enabled, explicitly, on every socket.
 *
 * There is intentionally no code path, debug flag or build variant that
 * relaxes any of this.
 */
class PinnedTrustManager(private val expected: Fingerprint) : X509TrustManager {

    override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
        val leaf = chain?.firstOrNull()
            ?: throw CertificateException("server presented no certificate")

        // Validity is a hygiene check, not the trust anchor. It runs so that a
        // long-forgotten certificate surfaces as a clear error rather than
        // working forever.
        try {
            leaf.checkValidity()
        } catch (e: Exception) {
            throw CertificateException("server certificate is not currently valid", e)
        }

        val presented = Fingerprint.ofCertificate(leaf)
        if (!presented.contentEquals(expected)) {
            // The message deliberately contains only fingerprints, which are
            // public values, and no addresses or user data.
            throw CertificateException(
                "server identity mismatch: expected ${expected.toDisplayShort()}, " +
                    "got ${presented.toDisplayShort()}",
            )
        }
    }

    override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {
        // This app is never a TLS server. Reaching here means a caller wired
        // the trust manager into a server socket by mistake; refusing loudly
        // is better than accepting a peer we have no policy for.
        throw CertificateException("this device does not accept inbound TLS connections")
    }

    // No CA is trusted, because there is no CA. Returning an empty array is
    // correct here and is not the same thing as trusting everything: the
    // check above is what decides.
    override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
}

/**
 * Presents this device's Keystore-backed identity as the TLS client
 * certificate.
 *
 * The private key object is a Keystore handle, not key material: signing
 * happens inside the TEE.
 */
class IdentityKeyManager(private val identity: DeviceIdentity) : X509ExtendedKeyManager() {

    private companion object {
        const val ALIAS = "omnibridge-identity"
    }

    override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?) =
        arrayOf(ALIAS)

    override fun chooseClientAlias(
        keyType: Array<out String>?,
        issuers: Array<out Principal>?,
        socket: Socket?,
    ): String {
        // Always the same identity, regardless of what the server hints. The
        // server has no CA to hint with, and there is only one identity.
        return ALIAS
    }

    override fun chooseEngineClientAlias(
        keyType: Array<out String>?,
        issuers: Array<out Principal>?,
        engine: javax.net.ssl.SSLEngine?,
    ): String = ALIAS

    override fun getCertificateChain(alias: String?): Array<X509Certificate> =
        arrayOf(identity.certificate)

    override fun getPrivateKey(alias: String?): PrivateKey = identity.privateKey

    override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? =
        null

    override fun chooseServerAlias(
        keyType: String?,
        issuers: Array<out Principal>?,
        socket: Socket?,
    ): String? = null
}

object TlsFactory {

    const val ALPN_PROTOCOL = "omnibridge/1"

    /**
     * ALPN for a `files.v1` bulk data stream (ADR-0012, ADR-0013).
     *
     * A data stream is a second TLS 1.3 connection to the *same* port with
     * the *same* pinned identity. ALPN is what tells the desktop which of the
     * two it just accepted, before a single application byte is read. It is
     * not a weaker connection — it is the same connection carrying different
     * traffic.
     */
    const val ALPN_DATA_PROTOCOL = "omnibridge-data/1"

    /**
     * Builds a socket factory that will accept exactly one server identity.
     */
    fun sslContext(identity: DeviceIdentity, pinned: Fingerprint): SSLContext {
        // "TLSv1.3" rather than "TLS": asking for the generic protocol would
        // let the context negotiate down if a future platform re-enabled
        // older versions by default.
        val context = SSLContext.getInstance("TLSv1.3")
        context.init(
            arrayOf(IdentityKeyManager(identity)),
            arrayOf(PinnedTrustManager(pinned)),
            java.security.SecureRandom(),
        )
        return context
    }

    /**
     * Applies the non-negotiable socket settings.
     *
     * Called on every socket before the handshake. Belt and braces on top of
     * the "TLSv1.3" context: if the platform ever hands back a context that
     * enables more, this narrows it again.
     */
    fun harden(socket: SSLSocket, alpn: String = ALPN_PROTOCOL) {
        socket.enabledProtocols = arrayOf("TLSv1.3")
        socket.sslParameters = socket.sslParameters.apply {
            applicationProtocols = arrayOf(alpn)
            // Endpoint identification is left off deliberately: it verifies
            // hostnames, and hostnames are not identity in this protocol
            // (principle 6). The pinning check in PinnedTrustManager is
            // strictly stronger, and our certificates carry no SANs for a
            // hostname check to even use.
            endpointIdentificationAlgorithm = null
        }
        socket.tcpNoDelay = true
    }
}
