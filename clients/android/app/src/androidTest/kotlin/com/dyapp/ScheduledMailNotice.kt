package com.dyapp

import android.app.ActivityManager
import android.app.Notification
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.view.accessibility.AccessibilityEvent
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

internal data class ObservedMailNotice(val slot: String, val peer: String, val notice: Notification)

/** A real framework/native pass: deadlines fail transitions; silence is checked only after completion. */
internal class ScheduledMailNotice(private val context: Context) : MailNoticeJobProbe, AutoCloseable {
    private val scheduler = context.getSystemService(JobScheduler::class.java)
    private val occupied = scheduler.allPendingJobs.map { it.id }.toSet() + setOf(MAIL_JOB_ID, SEARCH_OUTBOX_JOB_ID)
    private val jobId = generateSequence(0) { it + 1 }.first { it !in occupied }
    private val started = CompletableFuture<Unit>()
    private val finished = CompletableFuture<Pair<Boolean, Boolean>>()
    private val stops = AtomicInteger()
    val attempts = CopyOnWriteArrayList<ObservedMailNotice>()
    private var scheduled = false

    fun schedule() {
        requireNoMailActivities()
        val state = ActivityManager.RunningAppProcessInfo().also { ActivityManager.getMyMemoryState(it) }
        check(state.importance > ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND) {
            "Instrumentation is foreground; this cannot prove background notification policy"
        }
        check(noticesAllowed(context)) { "OS notification permission must stay enabled for native-policy cases" }
        check(mailNoticeJobProbes.putIfAbsent(null to jobId, this) == null) { "Another mail probe owns this job ID" }
        val job = JobInfo.Builder(jobId, ComponentName(context, MailNoticeProbeService::class.java))
            .setExpedited(true)
            .build()
        check(scheduler.schedule(job) == JobScheduler.RESULT_SUCCESS) { "Framework refused the mail notice job" }
        scheduled = true
    }

    /** Register the OS event listener before triggering the job, so fast posts cannot be missed. */
    fun awaitOsPost(body: String): Notification {
        val event = InstrumentationRegistry.getInstrumentation().uiAutomation.executeAndWaitForEvent(
            { schedule() },
            {
                val notice = it.parcelableData as? Notification
                it.eventType == AccessibilityEvent.TYPE_NOTIFICATION_STATE_CHANGED &&
                    it.packageName?.toString() == context.packageName &&
                    notice?.extras?.getCharSequence(Notification.EXTRA_TEXT)?.toString() == body
            },
            TimeUnit.SECONDS.toMillis(NETWORK_STEP_SECONDS)
        )
        return checkNotNull(event.parcelableData as? Notification).clone()
    }

    fun awaitCompletion() {
        started.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
        val completion = finished.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
        check(stops.get() == 0) { "Framework stopped the mail observation" }
        check(completion.first && !completion.second) { "Mail pass did not complete successfully: $completion" }
    }

    override fun started() { started.complete(Unit) }

    override fun attempted(slotId: String, peerId: String, notice: Notification) {
        attempts.add(ObservedMailNotice(slotId, peerId, notice))
    }

    override fun stopped() { stops.incrementAndGet() }

    override fun finished(completionRequested: Boolean, retryRequested: Boolean) {
        finished.complete(completionRequested to retryRequested)
    }

    override fun close() {
        mailNoticeJobProbes.remove(null to jobId, this)
        if (scheduled) scheduler.cancel(jobId)
        if (started.isDone) finished.get(NETWORK_STEP_SECONDS, TimeUnit.SECONDS)
    }
}

private fun requireNoMailActivities() {
    var foregroundConsumer = false
    InstrumentationRegistry.getInstrumentation().runOnMainSync {
        val monitor = ActivityLifecycleMonitorRegistry.getInstance()
        foregroundConsumer = Stage.values().any {
            it != Stage.DESTROYED && monitor.getActivitiesInStage(it).isNotEmpty()
        }
    }
    check(!foregroundConsumer) { "An Activity could receive mail instead of the JobService" }
}
