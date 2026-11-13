package com.dyapp

import android.content.Context
import uniffi.dyapp.ChatState
import uniffi.dyapp.ChatSubmission
import java.io.File

/** Real retained peers share only native messages; the active peer uses the worker's registry. */
internal class WorkerMailPair(private val context: Context) {
    private val worker = WorkerPeerFixture(
        context.noBackupFilesDir,
        File(context.filesDir, "worker-peer-state.json")
    )
    private val counterpart = WorkerPeerFixture(
        File(context.filesDir, "worker-counterpart"),
        File(context.filesDir, "worker-counterpart-state.json")
    )

    fun prepare(): Opened.Ready {
        val owner = worker.prepare()
        val other = counterpart.prepare()
        val record = record(owner, other)
        record.begin()
        publishWorkerPeer(owner)
        publishWorkerPeer(other)
        match(owner, other)
        val sent = RustBridge.withChats(owner) { it.submitText(other.peerId, WORKER_ANCHOR) }
        check(sent is ChatSubmission.Accepted) { "Online anchor was not accepted" }
        record.record(WORKER_ANCHOR, sent.id)
        receiveWorkerCounterpart(other, owner.peerId, sent.id, WORKER_ANCHOR)
        check(workerMessagePresent(owner, other.peerId, sent.id, WORKER_ANCHOR))
        return owner
    }

    fun reopen(): Opened.Ready {
        val owner = worker.reopen()
        val other = counterpart.reopen()
        val id = record(owner, other).id(WORKER_ANCHOR)
        check(workerMessagePresent(owner, other.peerId, id, WORKER_ANCHOR)) { "Owner lost the anchor" }
        check(workerMessagePresent(other, owner.peerId, id, WORKER_ANCHOR)) { "Counterpart lost the anchor" }
        return owner
    }

    fun queueOffline(): Opened.Ready {
        val owner = reopen()
        val other = counterpart.reopen()
        val sent = RustBridge.withChats(owner) { it.submitText(other.peerId, WORKER_OFFLINE) }
        check(sent is ChatSubmission.Queued) { "Offline operation must retain a committed ID" }
        record(owner, other).record(WORKER_OFFLINE, sent.id)
        return reopenQueued()
    }

    fun reopenQueued(): Opened.Ready {
        val owner = reopen()
        val other = counterpart.reopen()
        val id = record(owner, other).id(WORKER_OFFLINE)
        check(workerMessagePresent(owner, other.peerId, id, WORKER_OFFLINE)) { "Queued ID was lost" }
        check(!workerMessagePresent(other, owner.peerId, id, WORKER_OFFLINE)) {
            "Offline operation reached the counterpart before catch-up"
        }
        return owner
    }

    /** Calls the actual worker body; this does not assert JobScheduler or idle scheduling. */
    fun catchUpBody(): Opened.Ready {
        val owner = reopen()
        val other = counterpart.reopen()
        val record = record(owner, other)
        activateWorkerPeer(owner)
        val incoming = RustBridge.withChats(other) { it.submitText(owner.peerId, WORKER_INCOMING) }
        check(incoming is ChatSubmission.Accepted) { "Counterpart incoming message was not accepted" }
        record.record(WORKER_INCOMING, incoming.id)
        receiveMail(context)
        check(workerMessagePresent(owner, other.peerId, incoming.id, WORKER_INCOMING)) {
            "Worker body did not apply incoming mail"
        }
        val queued = record.id(WORKER_OFFLINE)
        receiveWorkerCounterpart(other, owner.peerId, queued, WORKER_OFFLINE)
        receiveMail(context)
        RustBridge.withChats(other) { it.receive() }
        check(workerMessagePresent(owner, other.peerId, incoming.id, WORKER_INCOMING))
        check(workerMessagePresent(other, owner.peerId, queued, WORKER_OFFLINE))
        return owner
    }

    fun cleanup() {
        val other = runCatching { counterpart.cleanup() }
        val owner = runCatching { worker.cleanup() }
        other.exceptionOrNull()?.let { failure ->
            owner.exceptionOrNull()?.let(failure::addSuppressed)
            throw failure
        }
        owner.getOrThrow()
    }

    private fun record(owner: Opened.Ready, other: Opened.Ready) =
        WorkerMailRecord(File(context.filesDir, "worker-mail-ids.json"), owner.peerId, other.peerId)

    private fun match(owner: Opened.Ready, other: Opened.Ready) {
        RustBridge.withChats(owner) { it.like(other.peerId, true) }
        RustBridge.withChats(other) { it.receive() }
        RustBridge.withChats(other) { it.like(owner.peerId, true) }
        RustBridge.withChats(owner) { it.receive() }
        check(RustBridge.withChats(owner) { it.state(other.peerId) } == ChatState.OPEN)
        check(RustBridge.withChats(other) { it.state(owner.peerId) } == ChatState.OPEN)
    }
}
