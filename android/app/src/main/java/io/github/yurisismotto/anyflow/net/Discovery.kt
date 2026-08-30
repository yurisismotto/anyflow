package io.github.yurisismotto.anyflow.net

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow

/**
 * Finds AnyFlow daemons on the local network via DNS-SD.
 *
 * ## Discovery is not trust
 *
 * Everything here is attacker-controlled: anyone on the link can publish a
 * record claiming any name, id or address. This class answers only "what is
 * worth dialling". The identity check happens in the TLS handshake against
 * the pinned key, and a spoofed record produces nothing but a failed
 * connection.
 */
class Discovery(context: Context) {

    private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val wifi = context.applicationContext
        .getSystemService(Context.WIFI_SERVICE) as WifiManager

    data class Found(
        val deviceId: String?,
        val deviceName: String?,
        val address: InetSocketAddress,
    )

    /**
     * Browses for services until the flow is cancelled.
     *
     * A multicast lock is held for the duration: without it, Wi-Fi hardware
     * filters multicast packets while the screen is off and discovery
     * silently stops finding anything.
     */
    fun browse(): Flow<Found> = callbackFlow {
        val lock = wifi.createMulticastLock("anyflow-discovery").apply {
            setReferenceCounted(true)
            acquire()
        }

        val resolveListener = object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo?, errorCode: Int) {
                Log.d(TAG, "resolve failed: $errorCode")
            }

            override fun onServiceResolved(info: NsdServiceInfo) {
                val attributes = info.attributes ?: emptyMap()
                val deviceId = attributes["id"]?.toString(Charsets.UTF_8)
                    ?.takeIf { it.length <= 64 && it.all { c -> c.isHex() } }
                val deviceName = attributes["dn"]?.toString(Charsets.UTF_8)
                    ?.let { name -> name.filter { !it.isISOControl() }.take(64) }

                // Emit every resolved address, not just one: a daemon on a
                // dual-stack link publishes both, and picking a single one
                // arbitrarily can pick the unreachable one.
                for (host in info.resolvedAddresses()) {
                    trySend(Found(deviceId, deviceName, InetSocketAddress(host, info.port)))
                }
            }
        }

        val discoveryListener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String?) = Unit
            override fun onDiscoveryStopped(serviceType: String?) = Unit
            override fun onStartDiscoveryFailed(serviceType: String?, errorCode: Int) {
                close(IllegalStateException("could not start discovery: $errorCode"))
            }
            override fun onStopDiscoveryFailed(serviceType: String?, errorCode: Int) = Unit
            override fun onServiceLost(info: NsdServiceInfo?) = Unit

            override fun onServiceFound(info: NsdServiceInfo) {
                if (info.serviceType?.contains(SERVICE_TYPE_SHORT) != true) return
                @Suppress("DEPRECATION")
                nsd.resolveService(info, resolveListener)
            }
        }

        nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discoveryListener)

        awaitClose {
            runCatching { nsd.stopServiceDiscovery(discoveryListener) }
            runCatching { if (lock.isHeld) lock.release() }
        }
    }

    private fun Char.isHex() = this in '0'..'9' || this in 'a'..'f'

    /**
     * Every address the service resolved to.
     *
     * `getHost()` was deprecated in API 34 in favour of `getHostAddresses()`,
     * and it only ever returned one address. Below API 34 one address is all
     * the platform offers.
     */
    private fun NsdServiceInfo.resolvedAddresses(): List<InetAddress> =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            hostAddresses
        } else {
            @Suppress("DEPRECATION")
            listOfNotNull(host)
        }

    companion object {
        private const val TAG = "Discovery"
        const val SERVICE_TYPE = "_anyflow._tcp."
        private const val SERVICE_TYPE_SHORT = "_anyflow"
    }
}
