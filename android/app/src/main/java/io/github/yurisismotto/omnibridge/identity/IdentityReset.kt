package io.github.yurisismotto.omnibridge.identity

import io.github.yurisismotto.omnibridge.notifications.NotificationSecret

/**
 * Destroying this device's identity, and everything whose lifetime is tied to
 * it.
 *
 * ## Why this exists rather than two calls at a call site
 *
 * `device_notification_secret` **must not outlive an identity reset**
 * (ADR-0016 §5), and the reason is specific. `notification_id` is
 * `HMAC(secret, platform_key)`, and the platform key contains the posting
 * app's package and uid — both stable for the life of an install. A peer that
 * recorded ids before a re-pair and saw the same ids afterwards could link the
 * old identity to the new one, which is exactly the correlation re-pairing
 * exists to break. Rotating the secret with the identity costs nothing,
 * because a reset already invalidates every mirror when the peer fingerprint
 * changes, and it closes the link.
 *
 * Two separate calls at a call site is how that guarantee gets lost: someone
 * adds a second reset path a year from now, deletes the keystore identity, and
 * never learns that a notification secret existed. One function makes the pair
 * inseparable, and its documentation is where that person will read why.
 *
 * ## A regenerated secret is a mirror reset, never a reconciliation
 *
 * The source does not attempt to translate old ids to new ones — it cannot,
 * and trying would require retaining state across the very event that was
 * supposed to clear it. The ordinary reconnect flow does the work: the next
 * `SyncMarker{BEGIN}` … `{END}` bracket names the currently-active
 * notifications under their new ids, and the sink removes every mirror for
 * that peer not named in the snapshot. The old mirrors close through a
 * mechanism that already exists for another reason.
 *
 * ## Status in N1
 *
 * There is no UI that resets the device identity today — [DeviceIdentity.delete]
 * has had no caller since it was written, and N1 adds none, because this wave
 * builds no UI. What N1 adds is the guarantee that when such a path is built,
 * it cannot forget the notification secret.
 */
object IdentityReset {

    /**
     * Destroys the device identity and the notification secret together.
     *
     * Every existing pairing becomes unusable and every notification identity
     * this device has published changes. Both are the point.
     *
     * Neither step throws: a reset that failed halfway would leave the device
     * in the one state this function exists to prevent — a new identity with
     * an old notification secret — so each is attempted independently and the
     * result is that as much as could be destroyed was.
     */
    fun resetDeviceIdentity(
        secretStore: NotificationSecret.Store = NotificationSecret.KeystoreSecretStore(),
    ) {
        DeviceIdentity.delete()
        NotificationSecret.destroy(secretStore)
    }
}
