package io.github.yurisismotto.omnibridge.ui

import androidx.compose.runtime.Immutable
import io.github.yurisismotto.omnibridge.files.FailureReason
import io.github.yurisismotto.omnibridge.files.FileTransferManager
import io.github.yurisismotto.omnibridge.files.OpenAction
import io.github.yurisismotto.omnibridge.files.TransferState
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.store.TrustStore

/**
 * What the Files screen shows, worked out away from any Composable.
 *
 * Everything on that screen is a claim about something that happened — this
 * file was sent, that one was declined, this one can be opened. Each of those
 * is decided here, in a plain function over plain data, so it can be asserted
 * on the JVM rather than inspected by eye on a tablet.
 *
 * The rules that must not decay, in one place because they were all once
 * broken somewhere:
 *
 *  * a row is identified by its [FileTransferManager.TransferUi.transferId]
 *    and never by a filename. Two files called `report.pdf` are two
 *    transfers, and UX-DEBT-01 was exactly the defect of treating them as
 *    one;
 *  * a peer is identified by fingerprint and never by the name it calls
 *    itself. Two computers called "Fedora" stay two rows;
 *  * a status word is only ever produced from a state the transfer state
 *    machine actually reached. There is no word here for something it cannot
 *    prove.
 */
object FilesMapping {

    /**
     * Which way the file went, as a word.
     *
     * A word rather than an arrow, because an arrow is not readable by a
     * screen reader and not distinguishable by someone who cannot separate
     * the two accent colours. The glyph in the UI is decoration on top of
     * this.
     */
    enum class Direction { SENT, RECEIVED }

    /**
     * Where a transfer got to, in the vocabulary a person sees.
     *
     * This is a narrowing of [TransferState] plus [FailureReason], not a
     * parallel state machine: every value below is reachable, and every
     * terminal state maps onto exactly one of them.
     */
    enum class FileStatus {
        /** The offer exists and nobody has answered it yet. */
        WAITING,
        SENDING,
        RECEIVING,

        /** Terminal success. The peer confirmed it has the bytes. */
        SENT,

        /** Terminal success. The hash matched and the file was published. */
        RECEIVED,

        /** Someone said no — at either end. Not an error. */
        DECLINED,

        /** Someone stopped it after it started. Not an error either. */
        CANCELLED,

        /** An offer was never answered. Only ever reported by the peer. */
        TIMED_OUT,

        /** The session went away underneath it. */
        DISCONNECTED,

        /** Anything else: a bad hash, no storage, an unusable offer. */
        FAILED,
        ;

        val isTerminal: Boolean
            get() = this != WAITING && this != SENDING && this != RECEIVING
    }

    /**
     * One line of the Files screen.
     *
     * Contains no `Uri` and no path — see [FileTransferManager.TransferUi].
     * [openAction] says whether a button belongs here; what it would open is
     * fetched at the tap.
     */
    @Immutable
    data class FileRow(
        /** The only identity this row has. */
        val transferId: String,
        /** The peer's real identity, for keying and disambiguation. */
        val peerHex: String,
        /** What to call that peer on screen. Never used to decide anything. */
        val peerLabel: String,
        val direction: Direction,
        val displayName: String,
        /** Null when the size is genuinely not known, never 0 as a stand-in. */
        val sizeBytes: Long?,
        val status: FileStatus,
        /** Only while bytes are moving, and only when a percentage means something. */
        val progressPercent: Int?,
        val openAction: OpenAction,
    ) {
        val isActive: Boolean get() = !status.isTerminal

        /** The list key. Namespaced like the rest — see [UiMapping.transferKey]. */
        val key: String get() = UiMapping.transferKey(transferId)
    }

    /** The whole screen, split the way it is drawn. */
    @Immutable
    data class FilesUi(
        val active: List<FileRow>,
        val recent: List<FileRow>,
    ) {
        val isEmpty: Boolean get() = active.isEmpty() && recent.isEmpty()
    }

    /**
     * Turns the manager's live list into the two sections the screen draws.
     *
     * @param peers used only to put a friendly name on a fingerprint. A peer
     *   that is not in the list — revoked mid-transfer, say — still gets a
     *   row; it is identified by its fingerprint either way, and dropping the
     *   row would be hiding a transfer that really happened.
     */
    fun build(
        transfers: List<FileTransferManager.TransferUi>,
        peers: List<TrustStore.TrustedPeer>,
    ): FilesUi {
        val labels = peerLabels(peers)
        // Kept paired with their source, because the ordinals that decide the
        // order live on the transfer and must not be copied onto the row: the
        // row is what a screenshot and a log line can reach, and a monotonic
        // counter is a count of everything this process has ever transferred.
        val paired = transfers.map { it to row(it, labels) }
        return FilesUi(
            // Newest first in both sections, by different ordinals on
            // purpose. `sequence` is when a transfer started and
            // `settledSequence` when it ended, and those are not the same
            // order: a large file offered first routinely finishes last.
            active = paired.filter { (_, row) -> row.isActive }
                .sortedByDescending { (transfer, _) -> transfer.sequence }
                .map { (_, row) -> row },
            recent = paired.filterNot { (_, row) -> row.isActive }
                .sortedByDescending { (transfer, _) ->
                    transfer.settledSequence ?: transfer.sequence
                }
                .map { (_, row) -> row },
        )
    }

    /** One transfer as one row. */
    fun row(
        transfer: FileTransferManager.TransferUi,
        labels: Map<String, String>,
    ): FileRow {
        val hex = transfer.peer.toHex()
        return FileRow(
            transferId = transfer.transferId,
            peerHex = hex,
            peerLabel = labels[hex] ?: transfer.peer.toDisplayShort(),
            direction = if (transfer.sending) Direction.SENT else Direction.RECEIVED,
            displayName = transfer.displayName,
            // A zero-byte file is a real thing with a real size; "not known"
            // is a different fact and is written as one.
            sizeBytes = transfer.sizeBytes.takeIf { it >= 0 },
            status = status(transfer),
            progressPercent = transfer.percentage
                ?.takeIf { transfer.state == TransferState.TRANSFERRING },
            openAction = transfer.openAction,
        )
    }

    /**
     * The status word, derived from the state machine and nothing else.
     *
     * Note what is not consulted: bytes transferred, whether a file exists,
     * how long ago it was. A transfer that moved every byte and then failed
     * its hash check is `FAILED`, and no amount of progress may promote it.
     */
    fun status(transfer: FileTransferManager.TransferUi): FileStatus =
        when (transfer.state) {
            TransferState.OFFERED, TransferState.WAITING_ACCEPT -> FileStatus.WAITING
            TransferState.TRANSFERRING, TransferState.VERIFYING ->
                if (transfer.sending) FileStatus.SENDING else FileStatus.RECEIVING
            TransferState.COMPLETED ->
                if (transfer.sending) FileStatus.SENT else FileStatus.RECEIVED
            // A cancellation is not an error and is never drawn as one. Which
            // *kind* it was is the difference between "they said no" and "you
            // stopped it", and the two are not interchangeable.
            TransferState.CANCELLED -> when (transfer.failure) {
                FailureReason.DECLINED_BY_USER -> FileStatus.DECLINED
                else -> FileStatus.CANCELLED
            }
            TransferState.FAILED -> when (transfer.failure) {
                FailureReason.TIMED_OUT -> FileStatus.TIMED_OUT
                FailureReason.TRANSPORT -> FileStatus.DISCONNECTED
                else -> FileStatus.FAILED
            }
        }

    /**
     * A display name for every peer, disambiguated where it has to be.
     *
     * Two computers may legitimately call themselves the same thing, and one
     * of them may be a computer the person does not think they have. Where a
     * name is shared, the established short fingerprint is appended — the
     * same presentation the pairing screen compares — and never an IP
     * address, which identifies a network position rather than a machine and
     * changes on its own.
     *
     * The map is keyed by fingerprint hex, so the *label* is the only thing
     * derived from a name. Nothing routes on what comes out of here.
     */
    fun peerLabels(peers: List<TrustStore.TrustedPeer>): Map<String, String> {
        val counts = peers.groupingBy { it.deviceName }.eachCount()
        return peers.associate { peer ->
            val label = if ((counts[peer.deviceName] ?: 0) > 1) {
                "${peer.deviceName} · ${peer.fingerprint.toDisplayShort()}"
            } else {
                peer.deviceName
            }
            peer.fingerprint.toHex() to label
        }
    }

    /** The label for one fingerprint, for surfaces that hold a single peer. */
    fun peerLabel(fingerprint: Fingerprint, peers: List<TrustStore.TrustedPeer>): String =
        peerLabels(peers)[fingerprint.toHex()] ?: fingerprint.toDisplayShort()
}
