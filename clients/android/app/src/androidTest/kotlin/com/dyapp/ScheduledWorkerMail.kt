package com.dyapp

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import java.io.File

/** Framework service execution smoke; this does not prove periodic or idle scheduling policy. */
internal class ScheduledWorkerMail(private val context: Context) {
    fun awaitDelivery(owner: Opened.Ready, peer: String, id: ByteArray, text: String) {
        requireNoActivities()
        val directory = RustBridge.withKey(owner) { dir, _ -> File(dir) }
        val scheduler = context.getSystemService(JobScheduler::class.java)
        val occupied = scheduler.allPendingJobs.map { it.id }.toSet() + setOf(MAIL_JOB_ID, SEARCH_OUTBOX_JOB_ID)
        val jobId = generateSequence(0) { it + 1 }.first { it !in occupied }
        val job = JobInfo.Builder(jobId, ComponentName(context, MailJob::class.java))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setExpedited(true)
            .build()
        MailStorageSignal(directory).use { signal ->
            try {
                check(scheduler.schedule(job) == JobScheduler.RESULT_SUCCESS) {
                    "The framework refused the mail service smoke job"
                }
                PeerTransition("scheduled delivery of $text").awaitSignal(
                    ready = { workerMessagePresent(owner, peer, id, text) },
                    advance = signal::await
                )
            } finally {
                scheduler.cancel(jobId)
            }
        }
    }

    private fun requireNoActivities() {
        var foregroundConsumer = false
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val monitor = ActivityLifecycleMonitorRegistry.getInstance()
            foregroundConsumer = Stage.values().any {
                it != Stage.DESTROYED && monitor.getActivitiesInStage(it).isNotEmpty()
            }
        }
        check(!foregroundConsumer) { "An Activity could receive mail instead of the JobService" }
    }
}
