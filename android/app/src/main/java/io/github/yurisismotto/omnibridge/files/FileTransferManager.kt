package io.github.yurisismotto.omnibridge.files

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.util.Log
import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.identity.DeviceIdentity
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.proto.capabilities.DataStreamAuth
import io.github.yurisismotto.omnibridge.proto.capabilities.DataStreamReady
import io.github.yurisismotto.omnibridge.proto.capabilities.DataStreamStatus
import io.github.yurisismotto.omnibridge.proto.capabilities.FileAccept
import io.github.yurisismotto.omnibridge.proto.capabilities.FileCancel
import io.github.yurisismotto.omnibridge.proto.capabilities.FileComplete
import io.github.yurisismotto.omnibridge.proto.capabilities.FileControl
import io.github.yurisismotto.omnibridge.proto.capabilities.FileFailed
import io.github.yurisismotto.omnibridge.proto.capabilities.FileOffer
import io.github.yurisismotto.omnibridge.proto.capabilities.FileReject
import io.github.yurisismotto.omnibridge.proto.capabilities.TransferFailureReason
import java.net.InetSocketAddress
import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Every `files.v1` transfer this phone is involved in.
 *
 * Mirrors `omnibridge_capability_files::TransferManager`, in the **dialer** role:
 * a phone is not a stable listener, so it opens the data stream in both
 * directions of transfer and proves the challenge the desktop issued. See
 * ADR-0013.
 *
 * ## What is enforced here
 *
 * | Property | Where |
 * | --- | --- |
 * | only a granted computer may transfer | [isAuthorized], re-checked per offer |
 * | the user approves every incoming file | [pendingOffers] + [respondToOffer] |
 * | a filename cannot name a location | [Filenames.sanitize] + [Downloads] |
 * | the bytes are the offered bytes | SHA-256 checked before publishing |
 * | nothing unbounded | the limits below |
 */
class FileTransferManager(
    private val context: Context,
    private val identity: DeviceIdentity,
    private val scope: CoroutineScope,
    /** Answers "is this computer allowed to transfer files right now?" */
    private val isAuthorized: suspend (Fingerprint) -> Boolean,
) {

    private val downloads = Downloads(context)
    private val random = SecureRandom()

    /** Live transfers, by lowercase-hex transfer id. */
    private val transfers = ConcurrentHashMap<String, Transfer>()

    /**
     * Hands out the ordering ordinals in [TransferUi.sequence].
     *
     * A counter rather than a clock: `System.currentTimeMillis()` can repeat
     * or go backwards, and two transfers offered in the same millisecond —
     * which the share sheet's multi-select does routinely — would then have
     * no defined order at all.
     */
    private val sequencer = AtomicLong(0)

    /** Where to reach the desktop for a data stream. Set once connected. */
    @Volatile
    private var transport: Transport? = null

    /** How to send a control message to the desktop. Set once connected. */
    @Volatile
    private var controlSink: (suspend (ByteString) -> Unit)? = null

    private val _transfers = MutableStateFlow<List<TransferUi>>(emptyList())

    /** Everything the UI shows. */
    val visible: StateFlow<List<TransferUi>> = _transfers.asStateFlow()

    private val _pending = MutableStateFlow<List<IncomingOffer>>(emptyList())

    /** Offers waiting on the user. Rendered as accept/reject cards. */
    val pendingOffers: StateFlow<List<IncomingOffer>> = _pending.asStateFlow()

    /** Fired when a transfer settles, so the service can notify. */
    var onSettled: ((TransferUi) -> Unit)? = null

    class Transport(
        val address: InetSocketAddress,
        val pinned: Fingerprint,
    )

    /** An offer the user has not answered yet. */
    data class IncomingOffer(
        val transferId: String,
        val peer: Fingerprint,
        /** Already sanitized: a raw peer string must never reach the UI. */
        val filename: String,
        val sizeBytes: Long,
        val mimeType: String,
        internal val answer: CompletableDeferred<Boolean>,
    )

    /**
     * One transfer, as the UI sees it.
     *
     * ## What is deliberately not here
     *
     * No `Uri`, in either direction. The Files screen is built entirely from
     * this type, so anything in it can reach a row, a log line or a
     * screenshot — and a `content://` URI is a capability that routinely
     * carries a document id or an account name inside it. What the screen
     * needs to know is whether opening is *possible* ([hasOpenTarget]), which
     * is a boolean; the URI itself stays inside the manager and is fetched by
     * [resolveOpen] at the instant of a tap, when the answer is still true.
     *
     * No SHA-256 either. The digest is how the bytes were verified, not
     * something a person needs, and a hash of a file the user chose is an
     * identifier for that file.
     */
    data class TransferUi(
        val transferId: String,
        /** The computer at the other end. Identity, not decoration. */
        val peer: Fingerprint,
        val filename: String,
        val sizeBytes: Long,
        val bytesTransferred: Long,
        val sending: Boolean,
        val state: TransferState,
        val failure: FailureReason?,
        /**
         * What the file was actually saved as, when that differs from what
         * was offered.
         *
         * MediaStore renames a second `photo.jpg` to `photo (1).jpg` rather
         * than overwriting, and a row that kept showing the offered name
         * would send someone looking for a file that is not there.
         */
        val savedName: String? = null,
        /** Peer-supplied for an incoming file; the provider's for an outgoing one. */
        val mimeType: String = "",
        /**
         * Whether OmniBridge still holds something it could open.
         *
         * Structural only — that a target was retained, not that it still
         * resolves. [resolveOpen] is what asks the platform.
         */
        val hasOpenTarget: Boolean = false,
        /**
         * Set once a check has *proved* the target unreachable: a lapsed URI
         * grant, or a received file that has since been deleted.
         *
         * Never inferred. A row does not get to claim a file is missing
         * because nobody has looked yet.
         */
        val accessLost: Boolean = false,
        /**
         * Creation order within this process, from a monotonic counter.
         *
         * Ordering is by this and never by filename, timestamp or map
         * iteration: two files called `report.pdf` must sort stably, a
         * `HashMap` has no order to offer, and a wall clock can go backwards.
         */
        val sequence: Long = 0,
        /** The same counter, stamped when the transfer became terminal. */
        val settledSequence: Long? = null,
    ) {
        /** Null for a zero-byte file, where a percentage means nothing. */
        val percentage: Int?
            get() = if (sizeBytes == 0L) null else {
                ((bytesTransferred.coerceAtMost(sizeBytes) * 100) / sizeBytes).toInt()
            }

        /** What the person should be shown this file is called. */
        val displayName: String get() = savedName ?: filename

        /** Whether, and how, this row may be opened. */
        val openAction: OpenAction
            get() = FilesOpen.decide(sending, state, hasOpenTarget, accessLost)
    }

    /** What [resolveOpen] found when it actually looked. */
    sealed interface OpenResolution {
        /** Hand these to Android. Valid as of a moment ago, not forever. */
        data class Ready(val uri: Uri, val mimeType: String) : OpenResolution

        data class Unavailable(val action: OpenAction) : OpenResolution
    }

    private inner class Transfer(
        val id: String,
        val idBytes: ByteArray,
        val peer: Fingerprint,
        val sending: Boolean,
        val filename: String,
        val sizeBytes: Long,
        val sha256: ByteArray,
        val mimeType: String,
        /** Sending only. Never sent to the peer; only its basename is. */
        val source: Uri?,
    ) {
        @Volatile var state: TransferState = if (sending) {
            TransferState.OFFERED
        } else {
            TransferState.WAITING_ACCEPT
        }

        @Volatile var failure: FailureReason? = null
        val bytes = AtomicLong(0)
        val cancelled = AtomicBoolean(false)

        /**
         * The single-use challenge, dropped once a stream has used it. A
         * second stream for the same transfer finds nothing to prove.
         */
        @Volatile var challenge: ByteArray? = null

        /** Receiving only: the invisible entry bytes are streaming into. */
        @Volatile var pending: Downloads.Pending? = null

        /**
         * What Open would open, or null when there is nothing.
         *
         * Sending: the URI the user shared in. Holding it is not holding a
         * permission — the grant it arrived with has its own lifetime and is
         * re-checked before every use. Receiving: the published MediaStore
         * item, which exists only after the hash matched.
         */
        @Volatile var openUri: Uri? = if (sending) source else null

        /** What MediaStore actually called the file, once it is published. */
        @Volatile var savedName: String? = null

        /** Set only by a check that looked and failed. See [resolveOpen]. */
        @Volatile var accessLost: Boolean = false

        /** Creation order. See [TransferUi.sequence]. */
        val sequence: Long = sequencer.incrementAndGet()

        @Volatile var settledSequence: Long? = null

        fun ui() = TransferUi(
            transferId = id,
            peer = peer,
            filename = filename,
            sizeBytes = sizeBytes,
            bytesTransferred = bytes.get(),
            sending = sending,
            state = state,
            failure = failure,
            savedName = savedName,
            mimeType = mimeType,
            hasOpenTarget = openUri != null,
            accessLost = accessLost,
            sequence = sequence,
            settledSequence = settledSequence,
        )
    }

    // -----------------------------------------------------------------------
    // Session wiring
    // -----------------------------------------------------------------------

    /**
     * Records how to reach the desktop, once a control session exists.
     *
     * The data stream dials the *same address and port* the control session
     * used — there is no second discovery mechanism and no second port.
     */
    fun attachTransport(address: InetSocketAddress, pinned: Fingerprint) {
        transport = Transport(address, pinned)
    }

    fun attachControl(send: suspend (ByteString) -> Unit) {
        controlSink = send
    }

    /**
     * The control session ended.
     *
     * Every live transfer fails: the data stream is a *separate* TCP
     * connection and would otherwise keep running, or sit in `transferring`
     * forever. A partial file is discarded rather than left as an invisible
     * pending row.
     */
    fun onSessionEnded() {
        controlSink = null
        transport = null
        for (transfer in transfers.values) {
            if (transfer.state.isActive) {
                finish(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sending
    // -----------------------------------------------------------------------

    /**
     * Offers a file the user shared into the app.
     *
     * Hashes it first, because the receiver must know the expected digest
     * before the first byte arrives — that is what makes its check a
     * verification rather than a comparison against whatever turned up.
     */
    suspend fun offer(peer: Fingerprint, uri: Uri): Result<String> = withContext(Dispatchers.IO) {
        if (!isAuthorized(peer)) {
            return@withContext Result.failure(
                IllegalStateException("this computer is not allowed to receive files"),
            )
        }
        if (controlSink == null) {
            return@withContext Result.failure(IllegalStateException("not connected"))
        }

        if (activeCount(peer) >= MAX_CONCURRENT_TRANSFERS) {
            return@withContext Result.failure(
                IllegalStateException("too many transfers at once"),
            )
        }

        val shared = SharedFile(context, uri)
        val filename = shared.displayName()
            ?: return@withContext Result.failure(
                IllegalStateException("that file has no name OmniBridge can send safely"),
            )

        val (size, digest) = try {
            shared.measure()
        } catch (e: Exception) {
            // Re-stated, not re-thrown. A platform exception from a content
            // provider routinely embeds the whole `content://` URI — and on
            // some providers the display name with it — so handing it to a
            // caller that will show or log it is how a shared file's name
            // reaches a place it was never meant to be. The cause is kept for
            // a debugger and is deliberately not part of the message.
            return@withContext Result.failure(
                IllegalStateException("that file could not be read", e),
            )
        }

        // A fresh identity for every attempt, minted from randomness alone.
        // Nothing about the file — its name, its URI, its digest — feeds into
        // it, so re-offering the same file after a decline produces a
        // genuinely new transfer that the desktop must approve on its own
        // merits. See [StreamAuth.newTransferId].
        val idBytes = StreamAuth.newTransferId(random)
        val id = StreamAuth.toHex(idBytes)

        val transfer = Transfer(
            id = id,
            idBytes = idBytes,
            peer = peer,
            sending = true,
            filename = filename,
            sizeBytes = size,
            sha256 = digest,
            mimeType = shared.mimeType(),
            source = uri,
        )
        transfers[id] = transfer
        publishState()

        Log.i(TAG, "offering ${transfer.filename} (${size}B) as ${id.take(8)}")

        val sent = sendControl(
            FileControl.newBuilder().setOffer(
                FileOffer.newBuilder()
                    .setTransferId(ByteString.copyFrom(idBytes))
                    .setFilename(filename)
                    .setSizeBytes(size)
                    .setMimeType(transfer.mimeType)
                    .setSha256(ByteString.copyFrom(digest))
                    .setTimestampUnixMs(System.currentTimeMillis())
                    .build(),
            ).build(),
        )
        if (!sent) {
            finish(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
            return@withContext Result.failure(IllegalStateException("not connected"))
        }

        Result.success(id)
    }

    // -----------------------------------------------------------------------
    // Inbound control messages
    // -----------------------------------------------------------------------

    /**
     * Handles one `files.v1` payload.
     *
     * [payload] is untrusted, attacker-controlled bytes. Every field is
     * validated here, at the capability boundary, before anything can act on
     * it.
     */
    suspend fun handleControl(peer: Fingerprint, payload: ByteString) {
        val message = FileControl.parseFrom(payload)
        when (message.bodyCase) {
            FileControl.BodyCase.OFFER -> onOffer(peer, message.offer)
            FileControl.BodyCase.ACCEPT -> onAccept(peer, message.accept)
            FileControl.BodyCase.READY -> onReady(peer, message.ready)
            FileControl.BodyCase.COMPLETE -> onComplete(peer, message.complete.transferId)
            FileControl.BodyCase.REJECT ->
                onPeerEnded(peer, message.reject.transferId, message.reject.reason)
            FileControl.BodyCase.CANCEL ->
                onPeerEnded(peer, message.cancel.transferId, message.cancel.reason)
            FileControl.BodyCase.FAILED ->
                onPeerEnded(peer, message.failed.transferId, message.failed.reason)
            else -> Log.w(TAG, "ignoring an empty files.v1 message")
        }
    }

    private suspend fun onOffer(peer: Fingerprint, offer: FileOffer) {
        val idBytes = offer.transferId.toByteArray()
        if (idBytes.size != StreamAuth.TRANSFER_ID_LENGTH) {
            Log.w(TAG, "files.v1 offer with a malformed transfer id")
            return
        }
        val id = idBytes.joinToString("") { "%02x".format(it) }

        // Authorization first, before any resource is committed. This is the
        // receiver-side check, and it does not consult anything the sender
        // said.
        if (!isAuthorized(peer)) {
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_NOT_AUTHORIZED)
            return
        }

        // A transfer id is single-use. Reusing one would let a peer collide
        // with a live transfer or resurrect a finished one.
        if (transfers.containsKey(id)) {
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_BAD_METADATA)
            return
        }

        if (offer.sha256.size() != SHA256_BYTES ||
            offer.filename.length > Filenames.MAX_FILENAME_BYTES * 4 ||
            offer.mimeType.length > MAX_MIME_TYPE_BYTES
        ) {
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_BAD_METADATA)
            return
        }
        if (offer.sizeBytes > MAX_FILE_BYTES || offer.sizeBytes < 0) {
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_LARGE)
            return
        }

        val filename = Filenames.sanitize(offer.filename)
        if (filename == null) {
            // Refused rather than renamed: inventing a name for a file whose
            // own name was hostile hides the attack from the user.
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_BAD_METADATA)
            return
        }

        if (activeCount(peer) >= MAX_CONCURRENT_TRANSFERS) {
            reject(idBytes, TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_MANY_TRANSFERS)
            return
        }

        val transfer = Transfer(
            id = id,
            idBytes = idBytes,
            peer = peer,
            sending = false,
            filename = filename,
            sizeBytes = offer.sizeBytes,
            sha256 = offer.sha256.toByteArray(),
            mimeType = offer.mimeType,
            source = null,
        )
        transfers[id] = transfer
        publishState()

        // The sanitized name is logged; the raw one never is, at any level.
        Log.i(TAG, "incoming ${transfer.filename} (${offer.sizeBytes}B) as ${id.take(8)}")

        // Asking the user must not block the session's message loop: pings,
        // battery updates and an unpair all have to keep working while a
        // prompt sits on screen.
        scope.launch { awaitApproval(transfer) }
    }

    private suspend fun awaitApproval(transfer: Transfer) {
        val answer = CompletableDeferred<Boolean>()
        val request = IncomingOffer(
            transferId = transfer.id,
            peer = transfer.peer,
            filename = transfer.filename,
            sizeBytes = transfer.sizeBytes,
            mimeType = transfer.mimeType,
            answer = answer,
        )
        _pending.value = _pending.value + request

        val accepted = withTimeoutOrNull(ACCEPT_TIMEOUT_MS) { answer.await() } ?: false
        _pending.value = _pending.value.filterNot { it.transferId == transfer.id }

        if (!accepted) {
            finishAndTell(transfer, TransferState.CANCELLED, FailureReason.DECLINED_BY_USER)
            return
        }

        // Re-checked after the user, not only before: a pairing can be
        // revoked while a prompt is on screen, and the answer that matters is
        // the one true at the moment we would act.
        if (!isAuthorized(transfer.peer)) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.REVOKED)
            return
        }

        // Open the invisible entry before telling the desktop we are ready,
        // so a full disk is discovered now rather than mid-stream.
        val pending = try {
            withContext(Dispatchers.IO) {
                downloads.beginWrite(transfer.filename, transfer.mimeType)
            }
        } catch (e: Exception) {
            Log.w(TAG, "could not open a download entry: ${e.javaClass.simpleName}")
            finishAndTell(transfer, TransferState.FAILED, FailureReason.STORAGE)
            return
        }
        transfer.pending = pending

        if (!transition(transfer, TransferState.TRANSFERRING)) return

        // No challenge from this side: the phone dials, so the desktop issues
        // the challenge and will send it in FILE_READY.
        val sent = sendControl(
            FileControl.newBuilder().setAccept(
                FileAccept.newBuilder()
                    .setTransferId(ByteString.copyFrom(transfer.idBytes))
                    .build(),
            ).build(),
        )
        if (!sent) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
        }
    }

    /** The desktop accepted a file we offered, and sent the challenge. */
    private suspend fun onAccept(peer: Fingerprint, accept: FileAccept) {
        val transfer = ownedActive(accept.transferId.toByteArray(), peer, sending = true) ?: return
        val challenge = accept.streamChallenge.toByteArray()
        if (challenge.size != StreamAuth.CHALLENGE_LENGTH) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.BAD_METADATA)
            return
        }
        transfer.challenge = challenge
        if (!transition(transfer, TransferState.TRANSFERRING)) return
        scope.launch { runStream(transfer) }
    }

    /** The desktop is ready to be dialled for a file it is sending us. */
    private suspend fun onReady(
        peer: Fingerprint,
        ready: io.github.yurisismotto.omnibridge.proto.capabilities.FileReady,
    ) {
        val transfer = ownedActive(ready.transferId.toByteArray(), peer, sending = false) ?: return
        val challenge = ready.streamChallenge.toByteArray()
        if (challenge.size != StreamAuth.CHALLENGE_LENGTH) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.BAD_METADATA)
            return
        }
        transfer.challenge = challenge
        scope.launch { runStream(transfer) }
    }

    /**
     * The desktop verified and stored a file we sent.
     *
     * Only meaningful for a transfer we are *sending*. A FILE_COMPLETE about
     * one we are receiving would be the sender claiming our verification
     * result, which is not its to claim.
     */
    private fun onComplete(peer: Fingerprint, rawId: ByteString) {
        val transfer = ownedActive(rawId.toByteArray(), peer, sending = true) ?: return
        // A duplicate finds the transfer already terminal, and `ownedActive`
        // has already returned null.
        finish(transfer, TransferState.COMPLETED, null)
    }

    private fun onPeerEnded(peer: Fingerprint, rawId: ByteString, reason: TransferFailureReason) {
        val id = rawId.toByteArray().joinToString("") { "%02x".format(it) }
        val transfer = transfers[id] ?: return
        if (!transfer.peer.contentEquals(peer)) return

        val mapped = fromProto(reason)
        // Terminal without a reply: answering a cancel with a cancel would
        // let two peers ping-pong for as long as the session lasted.
        finish(
            transfer,
            if (mapped.isCancellation) TransferState.CANCELLED else TransferState.FAILED,
            mapped,
        )
    }

    // -----------------------------------------------------------------------
    // The data stream
    // -----------------------------------------------------------------------

    private suspend fun runStream(transfer: Transfer) = withContext(Dispatchers.IO) {
        val transport = this@FileTransferManager.transport
        val challenge = transfer.challenge
        // Taking it here is what makes the challenge single-use.
        transfer.challenge = null

        if (transport == null || challenge == null) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
            return@withContext
        }

        val socket = try {
            DataStream.open(transport.address, identity, transport.pinned)
        } catch (e: Exception) {
            Log.w(TAG, "data stream failed to open: ${e.javaClass.simpleName}")
            finishAndTell(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
            return@withContext
        }

        try {
            val mac = StreamAuth.compute(
                challenge = challenge,
                // The desktop accepts streams and issued the challenge.
                acceptor = transport.pinned,
                dialer = identity.fingerprint,
                transferId = transfer.idBytes,
            )
            DataStream.writeFrame(
                socket.outputStream,
                DataStreamAuth.newBuilder()
                    .setProtocolVersion(
                        io.github.yurisismotto.omnibridge.net.Protocol.VERSION_MAX,
                    )
                    .setTransferId(ByteString.copyFrom(transfer.idBytes))
                    .setMac(ByteString.copyFrom(mac))
                    .build()
                    .toByteArray(),
            )

            val ready = DataStreamReady.parseFrom(DataStream.readFrame(socket.inputStream))
            if (ready.status != DataStreamStatus.DATA_STREAM_STATUS_READY) {
                Log.w(TAG, "the computer refused the data stream: ${ready.reason}")
                finishAndTell(transfer, TransferState.FAILED, fromProto(ready.reason))
                return@withContext
            }

            // A large file legitimately takes a while; the bound is on
            // silence, not on total duration.
            socket.soTimeout = DataStream.IDLE_TIMEOUT_MS

            if (transfer.sending) {
                sendBytes(transfer, socket)
            } else {
                receiveBytes(transfer, socket)
            }
        } catch (e: DataStream.TransferCancelled) {
            finishAndTell(transfer, TransferState.CANCELLED, FailureReason.CANCELLED_BY_USER)
        } catch (e: Exception) {
            Log.w(TAG, "data stream ended: ${e.javaClass.simpleName}")
            finishAndTell(transfer, TransferState.FAILED, FailureReason.TRANSPORT)
        } finally {
            runCatching { socket.close() }
        }
    }

    private suspend fun sendBytes(transfer: Transfer, socket: javax.net.ssl.SSLSocket) {
        val uri = transfer.source ?: run {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.STORAGE)
            return
        }
        SharedFile(context, uri).openStream().use { source ->
            DataStream.sendExactly(
                source = source,
                out = socket.outputStream,
                socket = socket,
                expected = transfer.sizeBytes,
                onProgress = { transfer.bytes.set(it); publishState() },
                isCancelled = { transfer.cancelled.get() },
            )
        }
        // Deliberately no state change: only the desktop's FILE_COMPLETE
        // completes a send. Declaring success because our own write finished
        // would report a file as delivered that the other side may have
        // rejected.
        Log.i(TAG, "sent ${transfer.id.take(8)}; awaiting the computer's verdict")
    }

    private suspend fun receiveBytes(transfer: Transfer, socket: javax.net.ssl.SSLSocket) {
        val pending = transfer.pending ?: run {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.STORAGE)
            return
        }

        val result = try {
            pending.stream.use { sink ->
                DataStream.receiveExactly(
                    input = socket.inputStream,
                    sink = sink,
                    expected = transfer.sizeBytes,
                    onProgress = { transfer.bytes.set(it); publishState() },
                    isCancelled = { transfer.cancelled.get() },
                )
            }
        } catch (e: DataStream.TransferCancelled) {
            finishAndTell(transfer, TransferState.CANCELLED, FailureReason.CANCELLED_BY_USER)
            return
        } catch (e: DataStream.TruncatedTransfer) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.INTEGRITY)
            return
        } catch (e: DataStream.OversizedTransfer) {
            finishAndTell(transfer, TransferState.FAILED, FailureReason.INTEGRITY)
            return
        }

        if (!transition(transfer, TransferState.VERIFYING)) return

        if (!StreamAuth.verify(transfer.sha256, result.sha256)) {
            Log.w(TAG, "${transfer.id.take(8)} failed its hash check; discarding it")
            // `finish` discards the pending entry, so the bad bytes are never
            // published under any name.
            finishAndTell(transfer, TransferState.FAILED, FailureReason.INTEGRITY)
            return
        }

        // Verified. Only now does it become a file the user can see.
        withContext(Dispatchers.IO) { downloads.publish(pending) }
        val actualName = downloads.displayName(pending.uri) ?: transfer.filename
        transfer.savedName = actualName
        // The published item is what Open opens. It is kept *because* it is
        // now published: before this line it was a pending row that no other
        // app could see and that `finish` would have deleted, and offering to
        // open unverified bytes is the thing IS_PENDING exists to prevent.
        //
        // Keeping it is not a new permission. OmniBridge inserted this row, so
        // MediaStore already lets it read the item back; nothing is granted
        // here that was not true while the bytes were being written.
        transfer.openUri = pending.uri
        transfer.pending = null

        if (!transition(transfer, TransferState.COMPLETED)) return
        publishState()
        Log.i(TAG, "received and verified ${transfer.id.take(8)} as $actualName")

        sendControl(
            FileControl.newBuilder().setComplete(
                FileComplete.newBuilder()
                    .setTransferId(ByteString.copyFrom(transfer.idBytes))
                    .build(),
            ).build(),
        )
    }

    // -----------------------------------------------------------------------
    // User actions
    // -----------------------------------------------------------------------

    /** Answers an offer shown in the UI. */
    fun respondToOffer(transferId: String, accept: Boolean) {
        _pending.value.firstOrNull { it.transferId == transferId }?.answer?.complete(accept)
    }

    /**
     * Cancels a transfer.
     *
     * The flag is polled between chunks by the copy loop, so it takes effect
     * while bytes are moving rather than at the end of the file.
     */
    fun cancel(transferId: String) {
        val transfer = transfers[transferId] ?: return
        if (!transfer.state.isActive) return
        transfer.cancelled.set(true)
        // Answer any prompt still on screen, so a cancel during the approval
        // window is not left waiting for a timeout.
        respondToOffer(transferId, false)
        scope.launch {
            finishAndTell(transfer, TransferState.CANCELLED, FailureReason.CANCELLED_BY_USER)
        }
    }

    // -----------------------------------------------------------------------
    // Opening a finished transfer
    // -----------------------------------------------------------------------

    /**
     * Works out, *now*, whether this transfer can be handed to Android to
     * open, and with what.
     *
     * ## Why this is asked at the tap and not at the draw
     *
     * Both answers can stop being true while a row is on screen. A received
     * file can be deleted from the Files app; an outgoing file's read grant
     * can lapse when the Activity that received the share goes away. A URI
     * captured when the list was built would be a stale answer that fails —
     * or, far worse, succeeds against something else — at the moment it is
     * used. So the row carries only [TransferUi.hasOpenTarget] and this
     * function does the real work against the live platform.
     *
     * ## The two checks are deliberately different
     *
     * *Sending* asks `checkUriPermission`, because the question is about a
     * **grant**: the file belongs to another app and OmniBridge's access to it
     * has a lifetime it does not control. This is the check that must never
     * be skipped, and the failure it reports is
     * [OpenAction.SourceUnavailable] — not "file missing", because the file
     * is almost certainly still there and OmniBridge simply may not look at it.
     *
     * *Receiving* asks MediaStore whether the item still exists, because
     * OmniBridge inserted that row and needs no grant to read it. The only way
     * it goes away is that somebody deleted the file, which is
     * [OpenAction.FileMissing].
     *
     * Neither check opens the file, and neither reads a byte of it.
     *
     * A proven failure is recorded on the transfer, so the row stops offering
     * a button that has just been shown not to work. It is never recorded
     * from a guess.
     */
    suspend fun resolveOpen(transferId: String): OpenResolution = withContext(Dispatchers.IO) {
        val transfer = transfers[transferId]
            ?: return@withContext OpenResolution.Unavailable(OpenAction.NotApplicable)

        val structural = FilesOpen.decide(
            sending = transfer.sending,
            state = transfer.state,
            hasTarget = transfer.openUri != null,
            accessLost = transfer.accessLost,
        )
        if (structural != OpenAction.Available) {
            return@withContext OpenResolution.Unavailable(structural)
        }

        val uri = transfer.openUri
            ?: return@withContext OpenResolution.Unavailable(
                FilesOpen.decide(transfer.sending, transfer.state, hasTarget = false),
            )

        if (!reachable(transfer.sending, uri)) {
            markAccessLost(transfer)
            return@withContext OpenResolution.Unavailable(
                if (transfer.sending) OpenAction.SourceUnavailable else OpenAction.FileMissing,
            )
        }

        // Most trustworthy first: what the provider says about the item as it
        // is stored now, then what the transfer recorded. A peer's claim is
        // never used raw — see [MimeTypes].
        val declared = runCatching { context.contentResolver.getType(uri) }.getOrNull()
        OpenResolution.Ready(uri, MimeTypes.firstUsable(declared, transfer.mimeType))
    }

    /**
     * Re-checks every finished transfer that still offers an Open, and marks
     * the ones that no longer do.
     *
     * Called when the Files screen comes back into view, which is when a
     * stale row would be seen. That is an event, not a timer: nothing here
     * polls, and a screen that is not being looked at costs nothing.
     */
    suspend fun refreshOpenTargets() = withContext(Dispatchers.IO) {
        var changed = false
        for (transfer in transfers.values) {
            if (!transfer.state.isTerminal || transfer.accessLost) continue
            val uri = transfer.openUri ?: continue
            if (!reachable(transfer.sending, uri)) {
                markAccessLost(transfer)
                changed = true
            }
        }
        if (changed) publishState()
    }

    /**
     * Whether the target behind [uri] can still be reached.
     *
     * Deliberately conservative in one direction only: an exception means
     * "could not establish that it is there", which is reported as not
     * reachable. Claiming reachability off a failed check is what would put a
     * button on a row that cannot work.
     */
    private fun reachable(sending: Boolean, uri: android.net.Uri): Boolean = runCatching {
        if (sending) {
            context.checkUriPermission(
                uri,
                android.os.Process.myPid(),
                android.os.Process.myUid(),
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            // Ask for the cheapest column there is. A deleted item returns no
            // row; nothing is read and no stream is opened.
            context.contentResolver
                .query(uri, arrayOf(android.provider.MediaStore.MediaColumns._ID), null, null, null)
                ?.use { it.moveToFirst() } == true
        }
    }.getOrDefault(false)

    private fun markAccessLost(transfer: Transfer) {
        if (transfer.accessLost) return
        transfer.accessLost = true
        // No URI, no filename, no path: which transfer and which direction is
        // everything a log needs to explain a row that stopped offering Open.
        Log.i(
            TAG,
            "open target for ${transfer.id.take(8)} is no longer reachable " +
                "(${if (transfer.sending) "sent" else "received"})",
        )
        publishState()
    }

    /** Drops finished transfers from the list the UI shows. */
    fun clearFinished() {
        transfers.entries.removeAll { it.value.state.isTerminal }
        publishState()
    }

    // -----------------------------------------------------------------------
    // State machine plumbing
    // -----------------------------------------------------------------------

    /**
     * The only place a transfer changes state.
     *
     * Refuses a transition that is not in the table, so a duplicate
     * FILE_COMPLETE, a late cancel, or a second stream for a finished
     * transfer are all rejected here rather than at each call site.
     */
    private fun transition(transfer: Transfer, next: TransferState): Boolean {
        synchronized(transfer) {
            if (!transfer.state.canTransitionTo(next)) {
                Log.d(TAG, "refused ${transfer.state} -> $next for ${transfer.id.take(8)}")
                return false
            }
            transfer.state = next
            if (next.isTerminal) transfer.settledSequence = sequencer.incrementAndGet()
        }
        publishState()
        return true
    }

    private fun finish(transfer: Transfer, next: TransferState, reason: FailureReason?) {
        synchronized(transfer) {
            if (!transfer.state.canTransitionTo(next)) return
            transfer.state = next
            if (reason != null) transfer.failure = reason
            if (next.isTerminal) transfer.settledSequence = sequencer.incrementAndGet()
        }
        transfer.cancelled.set(true)
        transfer.challenge = null

        // An offer card for a transfer that has already ended is a prompt
        // nobody can answer. It used to survive its own transfer: `_pending`
        // was cleared only when the person answered or the two-minute timeout
        // fired, so a session that dropped while an offer was on screen left
        // "Accept / Reject" sitting above a row that already said
        // Disconnected — two statements about one file, one of them false.
        //
        // Answering it was never dangerous: `transition` refuses to leave a
        // terminal state, so the accept failed closed. It was simply untrue,
        // and the Files screen shows both halves at once, which is where it
        // became visible. Withdrawing the card here also releases the
        // coroutine that was waiting on it.
        _pending.value.firstOrNull { it.transferId == transfer.id }?.let { offer ->
            _pending.value = _pending.value.filterNot { it.transferId == transfer.id }
            offer.answer.complete(false)
        }

        // An unpublished entry is unverified peer data. It must not survive as
        // an invisible row nobody can find or delete.
        transfer.pending?.let { pending ->
            transfer.pending = null
            // And it must not survive as something Open could reach: the two
            // are the same URI, and discarding the bytes while leaving a row
            // offering to open them is the one way this screen could point at
            // data that failed its hash check.
            if (transfer.openUri == pending.uri) transfer.openUri = null
            runCatching { downloads.discard(pending) }
        }

        publishState()
        onSettled?.invoke(transfer.ui())
        Log.i(TAG, "transfer ${transfer.id.take(8)} $next${reason?.let { " ($it)" } ?: ""}")
    }

    /** Ends a transfer and tells the desktop why. */
    private suspend fun finishAndTell(
        transfer: Transfer,
        next: TransferState,
        reason: FailureReason?,
    ) {
        val wasActive = transfer.state.isActive
        finish(transfer, next, reason)
        if (!wasActive || reason == null) return

        val body = if (reason.isCancellation) {
            FileControl.newBuilder().setCancel(
                FileCancel.newBuilder()
                    .setTransferId(ByteString.copyFrom(transfer.idBytes))
                    .setReason(toProto(reason))
                    .build(),
            ).build()
        } else {
            FileControl.newBuilder().setFailed(
                FileFailed.newBuilder()
                    .setTransferId(ByteString.copyFrom(transfer.idBytes))
                    .setReason(toProto(reason))
                    .build(),
            ).build()
        }
        sendControl(body)
    }

    private suspend fun reject(idBytes: ByteArray, reason: TransferFailureReason) {
        sendControl(
            FileControl.newBuilder().setReject(
                FileReject.newBuilder()
                    .setTransferId(ByteString.copyFrom(idBytes))
                    .setReason(reason)
                    .build(),
            ).build(),
        )
    }

    /**
     * Returns an active transfer that belongs to this peer and runs in the
     * given direction; null if any of that is untrue.
     *
     * The ownership check matters: a peer must only be able to answer for its
     * own transfers.
     */
    private fun ownedActive(
        rawId: ByteArray,
        peer: Fingerprint,
        sending: Boolean,
    ): Transfer? {
        if (rawId.size != StreamAuth.TRANSFER_ID_LENGTH) return null
        val id = StreamAuth.toHex(rawId)
        val transfer = transfers[id] ?: return null
        if (!transfer.peer.contentEquals(peer)) return null
        if (transfer.sending != sending) return null
        if (!transfer.state.isActive) return null
        return transfer
    }

    private fun activeCount(peer: Fingerprint): Int =
        transfers.values.count { it.peer.contentEquals(peer) && it.state.isActive }

    /**
     * Republishes the whole list.
     *
     * Ordered by [TransferUi.sequence], newest first. It used to be ordered
     * by filename, which put two unrelated transfers next to each other
     * because they happened to be called the same thing, and made the
     * position of a row change when an unrelated one arrived. Order is now a
     * property of when a transfer happened, which is what "Recent" means.
     */
    private fun publishState() {
        _transfers.value = transfers.values.map { it.ui() }.sortedByDescending { it.sequence }
    }

    /**
     * Sends one control message. Returns false when there is no session, which
     * every caller treats as a transport failure rather than ignoring.
     */
    private suspend fun sendControl(message: FileControl): Boolean {
        val sink = controlSink ?: return false
        return runCatching { sink(message.toByteString()) }.isSuccess
    }

    companion object {
        private const val TAG = "FileTransfer"

        /** Bounds open streams and pending entries per peer. */
        const val MAX_CONCURRENT_TRANSFERS = 4

        /** How long the user has to answer an offer. */
        const val ACCEPT_TIMEOUT_MS = 120_000L

        /** Largest single file this phone accepts. */
        const val MAX_FILE_BYTES = 16L * 1024 * 1024 * 1024

        const val SHA256_BYTES = 32
        const val MAX_MIME_TYPE_BYTES = 128

        fun toProto(reason: FailureReason): TransferFailureReason = when (reason) {
            FailureReason.DECLINED_BY_USER ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_DECLINED_BY_USER
            FailureReason.NOT_AUTHORIZED ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_NOT_AUTHORIZED
            FailureReason.TOO_MANY_TRANSFERS ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_MANY_TRANSFERS
            FailureReason.TOO_LARGE -> TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_LARGE
            FailureReason.BAD_METADATA ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_BAD_METADATA
            FailureReason.TIMED_OUT -> TransferFailureReason.TRANSFER_FAILURE_REASON_TIMED_OUT
            FailureReason.INTEGRITY -> TransferFailureReason.TRANSFER_FAILURE_REASON_INTEGRITY
            FailureReason.STORAGE -> TransferFailureReason.TRANSFER_FAILURE_REASON_STORAGE
            FailureReason.TRANSPORT -> TransferFailureReason.TRANSFER_FAILURE_REASON_TRANSPORT
            FailureReason.CANCELLED_BY_USER ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_CANCELLED_BY_USER
            FailureReason.REVOKED -> TransferFailureReason.TRANSFER_FAILURE_REASON_REVOKED
            FailureReason.UNKNOWN_TRANSFER ->
                TransferFailureReason.TRANSFER_FAILURE_REASON_UNKNOWN_TRANSFER
        }

        fun fromProto(reason: TransferFailureReason): FailureReason = when (reason) {
            TransferFailureReason.TRANSFER_FAILURE_REASON_DECLINED_BY_USER ->
                FailureReason.DECLINED_BY_USER
            TransferFailureReason.TRANSFER_FAILURE_REASON_NOT_AUTHORIZED ->
                FailureReason.NOT_AUTHORIZED
            TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_MANY_TRANSFERS ->
                FailureReason.TOO_MANY_TRANSFERS
            TransferFailureReason.TRANSFER_FAILURE_REASON_TOO_LARGE -> FailureReason.TOO_LARGE
            TransferFailureReason.TRANSFER_FAILURE_REASON_BAD_METADATA ->
                FailureReason.BAD_METADATA
            TransferFailureReason.TRANSFER_FAILURE_REASON_TIMED_OUT -> FailureReason.TIMED_OUT
            TransferFailureReason.TRANSFER_FAILURE_REASON_INTEGRITY -> FailureReason.INTEGRITY
            TransferFailureReason.TRANSFER_FAILURE_REASON_STORAGE -> FailureReason.STORAGE
            TransferFailureReason.TRANSFER_FAILURE_REASON_TRANSPORT -> FailureReason.TRANSPORT
            TransferFailureReason.TRANSFER_FAILURE_REASON_CANCELLED_BY_USER ->
                FailureReason.CANCELLED_BY_USER
            TransferFailureReason.TRANSFER_FAILURE_REASON_REVOKED -> FailureReason.REVOKED
            // An unspecified or unrecognised reason from a newer peer is not
            // an error in itself; it just tells us nothing.
            else -> FailureReason.UNKNOWN_TRANSFER
        }
    }
}
