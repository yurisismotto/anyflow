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
import io.github.yurisismotto.anyflow.net.ConnectionCoordinator
import io.github.yurisismotto.anyflow.net.DialResult
import io.github.yurisismotto.anyflow.net.Endpoints
import io.github.yurisismotto.anyflow.net.FailureKind
import io.github.yurisismotto.anyflow.net.LinkState
import io.github.yurisismotto.anyflow.net.PeerConnection
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.MainActivity
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
 * * The service runs only while the user has the connection enabled. It stops
 *   otherwise, rather than idling.
 * * Reconnection is capped exponential backoff with jitter, driven by
 *   [ConnectionCoordinator]. `ConnectivityManager` callbacks can bring a
 *   pending retry forward but are never the only thing that schedules one.
 *   There is no wakelock-and-spin loop, no alarm hammering, and nothing that
 *   fights Doze.
 * * No `START_STICKY` resurrection games and no boot receiver: if the system
 *   stops us, we stay stopped until the user brings us back.
 *
 * ## This class does not decide when to retry
 *
 * It owns the notification, the network callback and the platform lifecycle,
 * and it hands the coordinator two functions: where to dial and how. Keeping
 * the retry policy in one place — and out of the component that has four
 * different callback threads — is what fixes the loop that used to die.
 */
class ConnectionService : LifecycleService() {

    private lateinit var connectivity: ConnectivityManager
    private var coordinator: ConnectionCoordinator? = null

    /** The session currently running, so a lost network can end it promptly. */
    @Volatile
    private var currentConnection: PeerConnection? = null

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            coordinator?.onNetworkAvailable("wifi-or-ethernet")
        }

        override fun onLost(network: Network) {
            coordinator?.onNetworkLost("wifi-or-ethernet")
            // Closing the socket makes the blocked read fail now instead of
            // waiting out the idle timeout. It does not cancel anything: the
            // coordinator sees an ordinary session end and schedules a retry
            // exactly as it would for any other failure.
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
            // An explicit stop is not a failure and must never be retried.
            coordinator?.stop("user asked to disconnect")
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

        // Idempotent: a second start (the user pressing Connect again, or the
        // system redelivering) must not produce a second connection loop.
        ensureCoordinator().start()

        // Not sticky: being restarted by the system without the user asking
        // is exactly the background behaviour we are avoiding.
        return START_NOT_STICKY
    }

    @Synchronized
    private fun ensureCoordinator(): ConnectionCoordinator =
        coordinator ?: ConnectionCoordinator(
            scope = lifecycleScope,
            endpoints = ::endpointsFor,
            dial = ::dial,
            log = { event -> Log.i(TAG, event.toString()) },
            onState = ::onLinkState,
        ).also { coordinator = it }

    /** Where to dial this round. Empty when there is nothing to dial. */
    private suspend fun endpointsFor(round: Int) =
        pairedPeer()?.let { app.candidateAddresses(it, round) } ?: emptyList()

    /**
     * One dial, translated into the vocabulary the coordinator retries on.
     *
     * The distinction that matters: a transport failure is the network having
     * a bad day and is retried promptly; a security failure means the peer is
     * not the pinned identity and is retried slowly and never re-trusted; a
     * revocation is not retried at all.
     */
    private suspend fun dial(address: java.net.InetSocketAddress): DialResult {
        val peer = pairedPeer() ?: return DialResult.Terminal("no paired computer")

        return when (val result = app.connect(peer, address)) {
            is ConnectResult.Established -> DialResult.Established {
                val connection = result.connection
                currentConnection = connection
                updateNotification(getString(R.string.notif_connected, peer.deviceName))
                // A data stream reuses this session's address, port and
                // pinned identity. Recorded before the session runs, so a
                // transfer offered the instant we connect has somewhere to
                // dial.
                app.files.attachTransport(connection.remoteAddress, peer.fingerprint)
                // Remember only an address that actually worked, so the fast
                // path stays the one that was proven, not merely advertised.
                runCatching {
                    app.trustStore.rememberAddresses(
                        peer.fingerprint,
                        listOf(Endpoints.format(address)),
                    )
                }
                try {
                    connection.run(lifecycleScope)
                } finally {
                    currentConnection = null
                }
            }

            // The computer forgot us. Retrying cannot help and only the user
            // can fix it, so this ends the loop rather than backing off.
            is ConnectResult.PairingRequired ->
                DialResult.Terminal("the computer no longer knows this device")

            is ConnectResult.Failed -> when (result.kind) {
                FailureKind.REVOKED -> DialResult.Terminal(result.reason)
                FailureKind.SECURITY -> DialResult.Security(result.reason)
                FailureKind.TRANSPORT -> DialResult.Transient(result.reason)
            }
        }
    }

    private fun onLinkState(state: LinkState) {
        val app = app
        when (state) {
            is LinkState.Connecting -> {
                app.publishConnectionState(AnyFlowApp.ConnectionState.Connecting)
                updateNotification(getString(R.string.notif_connecting))
            }

            is LinkState.Connected -> {
                val peer = pairedPeer()
                app.publishConnectionState(
                    if (peer == null) {
                        AnyFlowApp.ConnectionState.Connecting
                    } else {
                        AnyFlowApp.ConnectionState.Connected(
                            peer.deviceName,
                            peer.fingerprint.toDisplayShort(),
                        )
                    },
                )
            }

            is LinkState.RetryWait -> {
                // The UI says "trying again", not "connected": a session that
                // ended must never keep looking live.
                app.publishConnectionState(
                    AnyFlowApp.ConnectionState.Retrying(state.reason, state.delayMs / 1000),
                )
                updateNotification(getString(R.string.notif_connecting))
            }

            is LinkState.GaveUp -> {
                app.publishConnectionState(AnyFlowApp.ConnectionState.Error(state.reason))
                // Nothing left to try, so holding a foreground service — and
                // its notification — would be claiming work we are not doing.
                lifecycleScope.launch { stopSelf() }
            }

            is LinkState.Stopped ->
                app.publishConnectionState(AnyFlowApp.ConnectionState.Idle)
        }
    }

    private val app: AnyFlowApp get() = application as AnyFlowApp

    private fun pairedPeer(): TrustStore.TrustedPeer? =
        runCatching { app.trustStore.peers().firstOrNull() }.getOrNull()

    override fun onDestroy() {
        // Being destroyed is an explicit end, not a failure: stop first so
        // nothing schedules a retry into a scope that is going away.
        coordinator?.stop("service destroyed")
        currentConnection?.disconnect()
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
        runCatching { manager.notify(NOTIFICATION_ID, buildNotification(text)) }
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
