package io.github.yurisismotto.omnibridge.fixture

import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.Gravity
import android.widget.ScrollView
import android.widget.TextView

/**
 * The OmniBridge notification fixture. **Test only.**
 *
 * One activity that can put a notification of a chosen shape on this device's
 * shade, on demand, from `adb`. It exists because every other way of doing
 * that on certification hardware is missing something the `notifications.v1`
 * gates need:
 *
 * | Route | What it cannot do |
 * | --- | --- |
 * | `cmd notification post` (`com.android.shell`) | no launcher entry, so the app picker cannot see it until it is already notifying; no cancel; no ongoing; no group; no progress |
 * | a real third-party app | not deterministic, and not something a suite may install |
 *
 * # What it can produce
 *
 * ```console
 * FX=io.github.yurisismotto.omnibridge.fixture/.FixtureActivity
 *
 * # a plain, clearable notification
 * adb shell am start -n $FX --es op post \
 *     --es id 1 --es tag n5 --es title 'Ana' --es body 'lunch?'
 *
 * # update it in place: same id and tag
 * adb shell am start -n $FX --es op post \
 *     --es id 1 --es tag n5 --es title 'Ana' --es body 'lunch at one?'
 *
 * # take it away
 * adb shell am start -n $FX --es op remove --es id 1 --es tag n5
 *
 * # an ONGOING notification: isClearable() is false, which is what the
 * # dismissal contract turns on
 * adb shell am start -n $FX --es op ongoing \
 *     --es id 2 --es tag n5-ongoing --es title 'Playing' --es body 'a track'
 *
 * # a grouped child and its summary
 * adb shell am start -n $FX --es op group --es id 3 --es tag n5-group
 *
 * # a progress notification, and one that expires by itself
 * adb shell am start -n $FX --es op progress --es id 4 --es progress 40
 * adb shell am start -n $FX --es op timeout --es id 5 --es timeout_ms 4000
 *
 * # everything this fixture put up, gone
 * adb shell am start -n $FX --es op clear
 * ```
 *
 * Every operation answers on logcat under the tag `OmniBridgeFixture`, with the
 * op, the id and the tag — never the title or the body, because this fixture
 * is used in the same runs as the logging canaries and a fixture that logged
 * its own payload would poison them.
 *
 * # Non-clearable, precisely
 *
 * `setOngoing(true)` sets `FLAG_ONGOING_EVENT`, and
 * `StatusBarNotification.isClearable()` is false whenever that flag or
 * `FLAG_NO_CLEAR` is set. `isClearable()` is exactly what OmniBridge's source
 * consults before honouring a `DismissRequest`, so this is the real gate and
 * not an approximation of it.
 *
 * What it is *not* is a promise about the shade: since Android 14 a person
 * can usually swipe away an ongoing notification posted by an app that is not
 * running a foreground service. That is a platform UI behaviour and it does
 * not change `isClearable()`. Producing a notification the *user* cannot
 * remove would need a foreground service, which needs `FOREGROUND_SERVICE`
 * and a service type — permissions this fixture deliberately does not hold,
 * for a distinction the code under test does not make.
 */
class FixtureActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ensureChannels()

        val op = intent?.getStringExtra(EXTRA_OP)
        if (op.isNullOrBlank()) {
            showUsage()
            return
        }
        perform(op)
        finish()
    }

    /** A relaunch with new extras is a new command, not a resume. */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        val op = intent?.getStringExtra(EXTRA_OP)
        if (!op.isNullOrBlank()) {
            perform(op)
            finish()
        }
    }

    // -----------------------------------------------------------------------
    // Operations
    // -----------------------------------------------------------------------

    private fun perform(op: String) {
        val id = intent.getStringExtra(EXTRA_ID)?.toIntOrNull() ?: DEFAULT_ID
        // An explicitly empty tag means "no tag", which is a distinct
        // identity from any tagged one and is worth being able to produce.
        val tag = intent.getStringExtra(EXTRA_TAG)?.takeIf { it.isNotEmpty() }
        val title = intent.getStringExtra(EXTRA_TITLE) ?: "OmniBridge fixture"
        val body = intent.getStringExtra(EXTRA_BODY) ?: "a test notification"

        when (op.lowercase()) {
            "post", "update" -> post(id, tag, title, body) {}

            "ongoing", "nonclearable" -> post(id, tag, title, body) {
                // FLAG_ONGOING_EVENT, and therefore isClearable() == false.
                it.setOngoing(true)
            }

            "progress" -> {
                val progress = intent.getStringExtra(EXTRA_PROGRESS)?.toIntOrNull() ?: 0
                post(id, tag, title, body) { it.setProgress(100, progress, false) }
            }

            "timeout" -> {
                val after = intent.getStringExtra(EXTRA_TIMEOUT_MS)?.toLongOrNull() ?: 5_000L
                post(id, tag, title, body) {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                        it.setTimeoutAfter(after)
                    }
                }
            }

            "group" -> {
                // A child and its summary, which is the shape a listener sees
                // for a conversation: two `StatusBarNotification`s, one of
                // them carrying FLAG_GROUP_SUMMARY.
                post(id, tag, title, body) { it.setGroup(GROUP_KEY) }
                post(id + 1, tag?.let { "$it-summary" }, "$title (group)", body) {
                    it.setGroup(GROUP_KEY).setGroupSummary(true)
                }
            }

            "remove", "cancel" -> {
                notifications().cancel(tag, id)
                Log.i(TAG, "op=remove id=$id tag=${tag ?: "-"}")
            }

            "clear" -> {
                notifications().cancelAll()
                Log.i(TAG, "op=clear")
            }

            else -> Log.w(TAG, "op=unknown value=${op.take(24)}")
        }
    }

    private fun post(
        id: Int,
        tag: String?,
        title: String,
        body: String,
        configure: (Notification.Builder) -> Unit,
    ) {
        val builder = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(false)
            .setWhen(System.currentTimeMillis())
            .setShowWhen(true)
        configure(builder)
        notifications().notify(tag, id, builder.build())
        // The id and the tag, never the content: this fixture runs in the
        // same sessions as NOTIF-SEC-25 and must not put a canary in logcat
        // itself.
        Log.i(TAG, "op=post id=$id tag=${tag ?: "-"} channel=$CHANNEL_ID")
    }

    // -----------------------------------------------------------------------
    // Plumbing
    // -----------------------------------------------------------------------

    private fun notifications(): NotificationManager =
        getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private fun ensureChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL_ID,
            "OmniBridge fixture",
            // DEFAULT rather than HIGH: a fixture that fired a heads-up
            // banner on every post would fight the very focus the
            // instrumented UI tests need.
            NotificationManager.IMPORTANCE_DEFAULT,
        ).apply {
            description = "Test notifications for OmniBridge certification"
            lightColor = Color.BLUE
        }
        notifications().createNotificationChannel(channel)
    }

    private fun showUsage() {
        val text = TextView(this).apply {
            setPadding(48, 48, 48, 48)
            textSize = 14f
            gravity = Gravity.START
            text = USAGE
        }
        setContentView(ScrollView(this).apply { addView(text) })
    }

    companion object {
        private const val TAG = "OmniBridgeFixture"
        const val CHANNEL_ID = "omnibridge-fixture"
        private const val GROUP_KEY = "omnibridge-fixture-group"
        private const val DEFAULT_ID = 1

        const val EXTRA_OP = "op"
        const val EXTRA_ID = "id"
        const val EXTRA_TAG = "tag"
        const val EXTRA_TITLE = "title"
        const val EXTRA_BODY = "body"
        const val EXTRA_PROGRESS = "progress"
        const val EXTRA_TIMEOUT_MS = "timeout_ms"

        private val USAGE = """
            OmniBridge notification fixture — test only.

            This app posts notifications on demand so that OmniBridge's
            notifications.v1 gates can be run on real hardware. It holds no
            network permission and no storage permission: it can put a
            notification on this device's shade and nothing else.

            Drive it from a computer:

              adb shell am start \
                -n io.github.yurisismotto.omnibridge.fixture/.FixtureActivity \
                --es op post --es id 1 --es tag n5 \
                --es title 'Ana' --es body 'lunch?'

            op = post | update | remove | ongoing | group | progress |
                 timeout | clear

            Uninstall it when the certification run is finished.
        """.trimIndent()
    }
}
