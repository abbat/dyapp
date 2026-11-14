package com.dyapp

import android.app.Application
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.util.Log
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSearchOutboxes
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.SearchOutboxWake

internal const val SEARCH_OUTBOX_JOB_ID = 2
private const val SEARCH_LOG_TAG = "dyapp"

@Volatile
private var outboxApplication: Application? = null

/** Process startup restores persisted queues, independently of the selected identity and app lock. */
internal fun startSearchOutboxJobs(application: Application) {
    outboxApplication = application
    wakeSearchOutboxes()
}

/** A finished queue-changing call wakes delivery even if the UI no longer consumes its result. */
internal fun <T> withSearchOutboxChange(action: () -> T): T = try {
    action()
} finally {
    wakeSearchOutboxes()
}

private fun wakeSearchOutboxes() {
    outboxApplication?.let { scheduleSearchOutbox(it, 0uL, replace = true) }
}

/** Rust inventories every owner and supplies deadline/retry policy; Kotlin only drives the OS. */
internal fun pollSearchOutboxes(context: Context): SearchOutboxWake =
    MobileIdentityRegistry(
        context.noBackupFilesDir.absolutePath,
        MobileSlotLayout.ANDROID,
        NativeSlotKeys
    ).use { registry ->
        MobileSearchOutboxes(registry).use { it.poll() }
    }

internal fun scheduleSearchOutboxWake(context: Context, wake: SearchOutboxWake, retryScheduled: Boolean) {
    when (wake) {
        SearchOutboxWake.Idle -> Unit
        SearchOutboxWake.Retry -> if (!retryScheduled) scheduleSearchOutbox(context, 0uL, replace = false)
        is SearchOutboxWake.At -> scheduleSearchOutbox(context, wake.delayMs, replace = true)
    }
}

private fun scheduleSearchOutbox(context: Context, delayMs: ULong, replace: Boolean) {
    try {
        enqueueSearchOutbox(context, delayMs, replace)
    } catch (failure: IllegalStateException) {
        Log.i(SEARCH_LOG_TAG, "Search wake unavailable: ${failure.javaClass.simpleName}")
    } catch (failure: SecurityException) {
        Log.i(SEARCH_LOG_TAG, "Search wake denied: ${failure.javaClass.simpleName}")
    }
}

private fun enqueueSearchOutbox(context: Context, delayMs: ULong, replace: Boolean) {
    val scheduler = context.getSystemService(JobScheduler::class.java)
    if (!replace && scheduler.getPendingJob(SEARCH_OUTBOX_JOB_ID) != null) return
    val job = JobInfo.Builder(SEARCH_OUTBOX_JOB_ID, ComponentName(context, MailJob::class.java))
        .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
        .setMinimumLatency(delayMs.coerceAtMost(JobInfo.MAX_BACKOFF_DELAY_MILLIS.toULong()).toLong())
        .setPersisted(true)
        .build()
    if (scheduler.schedule(job) != JobScheduler.RESULT_SUCCESS) {
        Log.i(SEARCH_LOG_TAG, "Search wake was not scheduled; periodic delivery will retry")
    }
}
