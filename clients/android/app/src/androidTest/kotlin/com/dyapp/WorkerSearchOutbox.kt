package com.dyapp

import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSearchOutbox
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.ProfileSearcher
import uniffi.dyapp.SearchOutboxStatus
import java.io.File

/** Prepares a real signed queue and records the native deadline for the reboot assertions. */
internal class WorkerSearchOutbox(private val context: Context, private val owner: Opened.Ready) {
    private val checkpoint = PeerCheckpoint(File(context.filesDir, "worker-search-outbox.json"))

    fun prepare() = withOutbox { outbox ->
        check(outbox.poll() == SearchOutboxStatus.Idle) { "Initial search publication was not acknowledged" }
        val tag = "outbox-${owner.peerId.takeLast(16).lowercase()}"
        val draft = loadDraft(owner).copy(interests = listOf(tag))
        saveDraft(owner, draft)
        solveWorkerProfile(owner)
        val published = withSearchOutboxChange {
            RustBridge.withChats(owner) { it.publishProfile(true) }
        }
        check(published.draft.interests == listOf(tag)) { "Updated profile was not accepted by the node" }
        val scheduled = requireScheduled(outbox.status())
        val scheduler = context.getSystemService(JobScheduler::class.java)
        val job = checkNotNull(scheduler.getPendingJob(SEARCH_OUTBOX_JOB_ID)) {
            "Queue-changing publication did not register the search job"
        }
        check(job.isPersisted && job.service == ComponentName(context, MailJob::class.java)) {
            "Search job must persist and target the production MailJob"
        }
        checkNotNull(job.requiredNetwork) { "Search job must wait for network availability" }
        checkpoint.write(
            JSONObject().put("owner", owner.peerId).put("readyAt", scheduled.readyAt.toString()).put("tag", tag)
        )
    }

    fun verifyNotIndexed(seeker: Opened.Ready) = withSearch(seeker) { searcher, filters ->
        check(completeSearch(searcher, filters).profiles.isEmpty()) {
            "Queued update reached the index before its deadline"
        }
    }

    fun awaitDelivery(seeker: Opened.Ready) {
        val directory = RustBridge.withKey(owner) { dir, _ -> File(dir) }
        MailStorageSignal(directory).use { signal ->
            requestDelivery()
            withOutbox { outbox ->
                PeerTransition("search acknowledgement").awaitSignal(
                    ready = { outbox.status() == SearchOutboxStatus.Idle },
                    advance = signal::await
                )
            }
        }
        verifyDelivered(seeker)
    }

    private fun requestDelivery() = runBlocking {
        val readyAt = checkpoint.read().getString("readyAt").toLong()
        val remaining = readyAt * 1_000L - System.currentTimeMillis()
        check(remaining <= 600_000L) { "Unexpected search publication deadline: $readyAt" }
        if (remaining > 0) delay(remaining)
        // The guest can reach the node but Android does not validate the guest network.
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        val command = "cmd jobscheduler run -f com.dyapp $SEARCH_OUTBOX_JOB_ID"
        val reply = ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command))
            .bufferedReader()
            .use { it.readText() }
        check(reply.contains("Running job")) { "Search job dispatch failed: $reply" }
    }

    private fun verifyDelivered(seeker: Opened.Ready) = withSearch(seeker) { searcher, filters ->
        val found = completeSearch(searcher, filters, owner.peerId).profiles.single()
        check(found.peerId == owner.peerId && found.draft.interests == listOf(filters.interest)) {
            "Search node did not return the queued projection for its owner"
        }
    }

    private fun withSearch(
        seeker: Opened.Ready,
        checkIndex: (ProfileSearcher, SearchFilters) -> Unit,
    ) {
        val tag = checkpoint.read().getString("tag")
        seeker.profileSearcher().use { searcher ->
            val filters = SearchFilters(emptyList())
            filters.offered = completeSearch(searcher, filters).offered
            check(19u in filters.offered) { "Worker node does not offer interest search" }
            filters.interest = tag
            checkIndex(searcher, filters)
        }
    }

    fun verifyWithAnotherSelection() {
        val alternate = WorkerPeerFixture(
            context.noBackupFilesDir,
            File(context.filesDir, "worker-search-alternate.json")
        )
        val selected = alternate.prepare()
        try {
            check(selected.peerId != owner.peerId) { "Expected a distinct active identity" }
            verify()
            WorkerSearchOutbox(context, selected).withOutbox { outbox ->
                check(outbox.status() == SearchOutboxStatus.Idle) {
                    "The selected identity unexpectedly acquired search work"
                }
            }
        } finally {
            alternate.cleanup()
        }
    }

    fun verify() = withOutbox { outbox ->
        val saved = checkpoint.read()
        check(saved.getString("owner") == owner.peerId) { "Search outbox owner changed" }
        val scheduled = requireScheduled(outbox.status())
        check(saved.getString("readyAt") == scheduled.readyAt.toString()) { "Search outbox deadline changed" }
    }

    private fun requireScheduled(status: SearchOutboxStatus): SearchOutboxStatus.Scheduled {
        check(status is SearchOutboxStatus.Scheduled) { "Expected a queued search publication, got $status" }
        return status
    }

    private fun <T> withOutbox(action: (MobileSearchOutbox) -> T): T =
        MobileIdentityRegistry(context.noBackupFilesDir.absolutePath, MobileSlotLayout.ANDROID, NativeSlotKeys)
            .use { registry -> MobileSearchOutbox(registry, owner.slot.id, owner.peerId).use(action) }
}
