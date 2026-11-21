package com.dyapp

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission
import java.io.File

/** The worker owns a real selected registry slot; the independent sender never receives for it. */
internal class WorkerStickerNoticePair(context: Context) : AutoCloseable {
    private val worker = WorkerPeerFixture(
        context.noBackupFilesDir,
        File(context.filesDir, "sticker-notice-worker.json")
    )
    private val counterpart = WorkerPeerFixture(
        File(context.filesDir, "sticker-notice-counterpart"),
        File(context.filesDir, "sticker-notice-counterpart.json")
    )
    private var ownerCreated = false
    private var otherCreated = false
    lateinit var owner: Opened.Ready
        private set
    private lateinit var other: Opened.Ready
    private lateinit var pack: ByteArray

    fun prepare() {
        owner = worker.prepare()
        ownerCreated = true
        other = counterpart.prepare()
        otherCreated = true
        publishWorkerPeer(owner)
        publishWorkerPeer(other)
        RustBridge.withChats(owner) { it.like(other.peerId, true) }
        RustBridge.withChats(other) { it.receive() }
        RustBridge.withChats(other) { it.like(owner.peerId, true) }
        RustBridge.withChats(owner) { it.receive() }
        check(RustBridge.withChats(owner) { it.state(other.peerId) } == ChatState.OPEN)
        check(RustBridge.withChats(other) { it.state(owner.peerId) } == ChatState.OPEN)
        val imported = RustBridge.withChats(other) { it.importStickerPack(archive()) }
        check(imported.added)
        pack = imported.packId
        RustBridge.withChats(owner) { it.setNotice(ChatNotice.TEXT) }
    }

    fun retainPrivateText() {
        val sent = RustBridge.withChats(other) { it.submitText(owner.peerId, "older private words") }
        check(sent is ChatSubmission.Accepted) { "Private-text fixture was not accepted by the node" }
        PeerTransition("owner retains earlier private text").await(
            ready = { workerMessagePresent(owner, other.peerId, sent.id, "older private words") },
            advance = { RustBridge.withChats(owner) { it.receive() } }
        )
        val last = RustBridge.withChats(owner) { it.chatRows() }.single { it.peerId == other.peerId }.lastMessage
        check(last?.unread == true) { "The private-text fixture must remain unread" }
    }

    fun sendSticker(): ByteArray {
        val sent = RustBridge.withChats(other) { it.submitSavedSticker(owner.peerId, pack, 0u) }
        check(sent is ChatSubmission.Accepted) { "Incoming sticker was not accepted by the node" }
        return sent.id
    }

    fun setMuted(muted: Boolean) = RustBridge.withChats(owner) { it.setMuted(other.peerId, muted) }

    fun requireReceived(id: ByteArray) {
        val stickers = RustBridge.withChats(owner) { it.chatItems(other.peerId) }
            .filterIsInstance<uniffi.dyapp.ChatItem.Sticker>()
        check(stickers.count { it.sticker.id.contentEquals(id) && it.sticker.author == other.peerId } == 1) {
            "The completed framework pass did not store this sender's exact sticker"
        }
        check(RustBridge.withChats(owner) { it.savedStickerPacks(null).entries.isEmpty() }) {
            "Notification receipt unexpectedly saved the sender's pack"
        }
    }

    override fun close() {
        val senderCleanup = runCatching { if (otherCreated) counterpart.cleanup() }
        val ownerCleanup = runCatching { if (ownerCreated) worker.cleanup() }
        senderCleanup.exceptionOrNull()?.let { failure ->
            ownerCleanup.exceptionOrNull()?.let(failure::addSuppressed)
            throw failure
        }
        ownerCleanup.getOrThrow()
    }

    private fun archive(): ByteArray =
        InstrumentationRegistry.getInstrumentation().context.assets.open("sticker-pack.tar").use { it.readBytes() }
}
