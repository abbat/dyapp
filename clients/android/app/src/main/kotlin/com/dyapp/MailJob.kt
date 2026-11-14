package com.dyapp

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.util.Log
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.CoreException
import uniffi.dyapp.SearchOutboxWake
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.thread

internal const val MAIL_JOB_ID = 1
private const val MAIL_LOG_TAG = "dyapp"

// The shortest period JobScheduler allows; Doze and App Standby stretch it further.
private const val MAIL_JOB_PERIOD_MS = 15 * 60_000L

/**
 * Receives the active identity's mail while the app is not on screen. The identity key is not
 * bound to a user check, so this also runs while the app lock is on; the lock only covers the UI.
 */
open class MailJob : JobService() {
    private val running = ConcurrentHashMap<Pair<String?, Int>, JobCompletion>()

    override fun onStartJob(params: JobParameters): Boolean {
        val completion = JobCompletion()
        running.put(params.scopedJobId(), completion)?.stop()
        thread(name = "mail-job") { runJob(params, completion) }
        return true
    }

    // Ending the OS task never releases an in-flight native admission or interrupts its worker.
    override fun onStopJob(params: JobParameters): Boolean {
        running.remove(params.scopedJobId())?.stop()
        return isSearchJob(params)
    }

    private fun runJob(params: JobParameters, completion: JobCompletion) {
        var completionRequested = false
        var retryRequested = false
        try {
            if (!isSearchJob(params)) receiveMail(applicationContext)
            if (!completion.isActive()) return
            val wake = searchPass(params)
            completion.complete {
                if (running.remove(params.scopedJobId(), completion)) {
                    val retry = isSearchJob(params) && wake == SearchOutboxWake.Retry
                    jobFinished(params, retry)
                    completionRequested = true
                    retryRequested = retry
                    scheduleSearchOutboxWake(applicationContext, wake, retryScheduled = retry)
                }
            }
        } finally {
            workerFinished(params, completionRequested, retryRequested)
        }
    }

    /** The framework adapter can be observed by a debug-only service without replacing its lifecycle. */
    protected open fun workerFinished(params: JobParameters, completionRequested: Boolean, retryRequested: Boolean) {
        // The release service has no observer.
    }

    protected open fun isSearchJob(params: JobParameters): Boolean = params.jobId == SEARCH_OUTBOX_JOB_ID

    protected open fun searchPass(params: JobParameters): SearchOutboxWake = try {
        pollSearchOutboxes(applicationContext)
    } catch (failure: CoreException) {
        Log.i(MAIL_LOG_TAG, "Search delivery job ${params.jobId} failed: ${failure.javaClass.simpleName}")
        SearchOutboxWake.Retry
    }
}

/** Namespace is part of framework job identity on API 34+, including stop callbacks. */
internal fun JobParameters.scopedJobId(): Pair<String?, Int> {
    val namespace = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) jobNamespace else null
    return namespace to jobId
}

/**
 * One receive for the active ready identity, then a notice when it left more to read; none, a
 * recovery state or a failure stays quiet.
 */
internal fun receiveMail(context: Context) {
    runCatching {
        val opened = RustBridge.slots(context).open() as? Opened.Ready ?: return
        RustBridge.withChats(opened) {
            val old = it.chatRows()
            val before = noticeCount(it, old)
            it.receive()
            val new = it.chatRows()
            val notice = it.notice()
            if (noticeCount(it, new) > before && notice != ChatNotice.OFF) {
                postMailNotice(context, noticeLines(notice, old, new))
            }
        }
    }.onFailure { Log.i(MAIL_LOG_TAG, "Background receive failed: ${it.javaClass.simpleName}") }
}

/** Schedules [MailJob] once; it survives reboots and runs only with a network. */
internal fun scheduleMailJob(context: Context) {
    val scheduler = context.getSystemService(JobScheduler::class.java)
    if (scheduler.getPendingJob(MAIL_JOB_ID) != null) return
    val job = JobInfo.Builder(MAIL_JOB_ID, ComponentName(context, MailJob::class.java))
        .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
        .setPeriodic(MAIL_JOB_PERIOD_MS)
        .setPersisted(true)
        .build()
    scheduler.schedule(job)
}
