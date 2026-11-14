package com.dyapp

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.CoreException
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSearchOutboxes
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.SearchOutboxWake
import java.nio.file.Files
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write

/** Own a free framework job ID and real native identity; never touch the app's selected slot. */
internal class SearchOutboxServiceFixture : SearchOutboxJobProbe, AutoCloseable {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val root = Files.createTempDirectory(context.cacheDir.toPath(), "outbox-job-").toFile()
    val gate = NetworkAdmissionGate()
    private val registry = MobileIdentityRegistry(root.absolutePath, MobileSlotLayout.ANDROID, gate)
    private val selected = registry.create()
    private val peer = selected.key.use { it.peerId() }
    private val outboxes = MobileSearchOutboxes(registry)
    private val scheduler = context.getSystemService(JobScheduler::class.java)
    private val occupied = scheduler.allPendingJobs.map { it.id }.toSet() + setOf(MAIL_JOB_ID, SEARCH_OUTBOX_JOB_ID)
    private val jobId = generateSequence(0) { it + 1 }.first { it !in occupied }
    private val access = ReentrantReadWriteLock(true)
    private var closed = false
    private val stopped = CompletableFuture<Unit>()
    private val finished = CompletableFuture<Boolean>()
    private val failure = AtomicReference<CoreException?>()
    val completions = AtomicInteger()
    val wake = AtomicReference<SearchOutboxWake?>()

    fun schedule() {
        check(searchOutboxJobProbe.compareAndSet(null, this)) { "Another outbox service probe is installed" }
        gate.arm()
        val job = JobInfo.Builder(jobId, ComponentName(context, SearchOutboxProbeService::class.java))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setExpedited(true)
            .build()
        check(scheduler.schedule(job) == JobScheduler.RESULT_SUCCESS) { "Framework refused the outbox probe" }
    }

    override fun poll(): SearchOutboxWake = access.read {
        if (closed) return@read SearchOutboxWake.Idle
        try {
            outboxes.poll().also { wake.set(it) }
        } catch (error: CoreException) {
            failure.set(error)
            SearchOutboxWake.Retry
        }
    }

    fun stop() {
        scheduler.cancel(jobId)
        stopped.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
    }

    override fun stopped() { stopped.complete(Unit) }

    override fun finished(completionRequested: Boolean) {
        if (completionRequested) completions.incrementAndGet()
        finished.complete(completionRequested)
    }

    fun awaitFinished(): Boolean {
        val completionRequested = finished.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
        gate.awaitOpened()
        failure.get()?.let { throw it }
        return completionRequested
    }

    fun requireAdmissionReleased() {
        val released = CompletableFuture<Unit>()
        kotlin.concurrent.thread(name = "outbox-admission-release", isDaemon = true) {
            runCatching { registry.select(selected.slot.id).key.use {} }
                .fold({ released.complete(Unit) }, { released.completeExceptionally(it) })
        }
        released.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
    }

    override fun close() {
        gate.release()
        scheduler.cancel(jobId)
        access.write {
            closed = true
            outboxes.close()
            check(registry.remove(selected.slot.id, peer) == null) { "Fixture cleanup persistence is unconfirmed" }
            registry.close()
        }
        searchOutboxJobProbe.compareAndSet(this, null)
        root.deleteRecursively()
    }
}
