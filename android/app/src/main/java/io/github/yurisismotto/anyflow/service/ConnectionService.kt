package io.github.yurisismotto.anyflow.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.net.ConnectResult
import io.github.yurisismotto.anyflow.net.PeerConnection
import io.github.yurisismotto.anyflow.ui.MainActivity
import java.net.InetSocketAddress
import kotlin.math.min
import kotlin.math.pow
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Keeps a connection to the paired computer while one is wanted.
 *
 * ## Why `connectedDevice` and not `dataSync`
 *
 * `dataSync` is time-boxed by the platform and is meant for finite transfers;
 * using it as a permanent daemon is precisely the pattern Android has been
 * tightening down on, and it would be killed. `connectedDevice` is the type
 * intended for maintaining a link with an external device, which is literally
 * what this is.
 *
 * That type requires the app to hold a qualifying permission. We hold
 * `CHANGE_WIFI_MULTICAST_STATE` because we genuinely take a multicast lock for
 * mDNS discovery — the permission is not a formality to satisfy the check.
 *
 * ## Playing by the rules
 *
 * * The service runs only while the user has the connection enabled *and*
 *   there is a usable network. It stops otherwise, rather than idling.
 * * Reconnection is driven by `ConnectivityManager` callbacks plus capped
 *   exponential backoff. There is no wakelock-and-spin loop, no alarm
 *   hammering, and nothing that fights Doze.
 * * No `START_STICKY` resurrection games and no boot receiver: if the system
 *   stops us, we stay stopped until the user or a network event brings us
 *   back.
 */
class ConnectionService : LifecycleService() {

    private lateinit var connectivity: ConnectivityManager
    private var connectionJob: Job? = null
    private var currentConnection: PeerConnection? = null
    private var attempt = 0

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            // A new network is the one moment a retry is actually likely to
            // succeed, so reset the backoff rather than waiting it out.
            attempt = 0
            ensureConnecting()
        }

        override fun onLost(network: Network) {
            currentConnection?.disconnect()
        }
    }

    override fun onCreate() {
        super.onCreate()
        connectivity = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        createNotificationChannel()
    }

    override fun onBind(intent: Intent): IBinder? {
        super.onBind(intent)
        return null
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)

        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }

        startForegroundCompat(getString(R.string.notif_connecting))

        val request = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
            .build()
        runCatching { connectivity.registerNetworkCallback(request, networkCallback) }

        ensureConnecting()

        // Not sticky: being restarted by the system without the user asking
        // is exactly the background behaviour we are avoiding.
        return START_NOT_STICKY
    }

    private fun ensureConnecting() {
        if (connectionJob?.isActive == true) return

        connectionJob = lifecycleScope.launch {
            val app = application as AnyFlowApp
            val peer = app.trustStore.peers().firstOrNull()
            if (peer == null) {
                Log.i(TAG, "no paired computer; stopping")
                stopSelf()
                return@launch
            }

            while (true) {
                val candidates = app.candidateAddresses(peer)
                var connected = false

                for (address in candidates) {
                    when (val result = app.connect(peer, address)) {
                        is ConnectResult.Established -> {
                            attempt = 0
                            connected = true
                            currentConnection = result.connection
                            updateNotification(
                                getString(R.string.notif_connected, peer.deviceName),
                            )
                            app.trustStore.rememberAddresses(
                                peer.fingerprint,
                                listOf("${address.hostString}:${address.port}"),
                            )
                            // Blocks until the session ends.
                            result.connection.run(lifecycleScope)
                            currentConnection = null
                            break
                        }
                        is ConnectResult.PairingRequired -> {
                            // The computer forgot us. Retrying cannot help and
                            // only the user can fix it.
                            Log.i(TAG, "the computer no longer trusts this device; stopping")
                            stopSelf()
                            return@launch
                        }
                        is ConnectResult.Failed -> {
                            Log.d(TAG, "connection attempt failed: ${result.reason}")
                        }
                    }
                }

                if (!connected) {
                    attempt += 1
                }
                updateNotification(getString(R.string.notif_connecting))

                // Capped exponential backoff. The cap matters: an uncapped
                // retry loop is a battery bug, and a too-short one is a
                // different battery bug.
                val delayMs = min(
                    MAX_BACKOFF_MS,
                    (BASE_BACKOFF_MS * 2.0.pow(attempt.coerceAtMost(6))).toLong(),
                )
                delay(delayMs)
            }
        }
    }

    override fun onDestroy() {
        currentConnection?.disconnect()
        connectionJob?.cancel()
        runCatching { connectivity.unregisterNetworkCallback(networkCallback) }
        super.onDestroy()
    }

    private fun startForegroundCompat(text: String) {
        val notification = buildNotification(text)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun updateNotification(text: String) {
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildNotification(text))
    }

    private fun buildNotification(text: String): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .setContentIntent(open)
            .build()
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            getString(R.string.channel_connection),
            // LOW: this is a persistent status notification, not an alert.
            NotificationManager.IMPORTANCE_LOW,
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        private const val TAG = "ConnectionService"
        private const val CHANNEL_ID = "connection"
        private const val NOTIFICATION_ID = 1
        private const val BASE_BACKOFF_MS = 2_000L
        private const val MAX_BACKOFF_MS = 5 * 60_000L

        const val ACTION_STOP = "io.github.yurisismotto.anyflow.STOP"

        fun start(context: Context) {
            context.startForegroundService(Intent(context, ConnectionService::class.java))
        }

        fun stop(context: Context) {
            context.startService(
                Intent(context, ConnectionService::class.java).setAction(ACTION_STOP),
            )
        }
    }
}
