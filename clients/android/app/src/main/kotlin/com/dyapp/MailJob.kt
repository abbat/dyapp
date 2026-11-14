package com.dyapp

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
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
class MailJob : JobService() {
    private val running = ConcurrentHashMap<Int, JobCompletion>()

    override fun onStartJob(params: JobParameters): Boolean {
        val completion = JobCompletion()
        running.put(params.jobId, completion)?.stop()
        thread(name = "mail-job") {
            if (params.jobId == MAIL_JOB_ID) receiveMail(applicationContext)
            if (!completion.isActive()) return@thread
            val wake = searchPass()
            completion.complete {
                if (running.remove(params.jobId, completion)) {
                    val retry = params.jobId == SEARCH_OUTBOX_JOB_ID && wake == SearchOutboxWake.Retry
                    jobFinished(params, retry)
                    scheduleSearchOutboxWake(applicationContext, wake, retryScheduled = retry)
                }
            }
        }
        return true
    }

    // Ending the OS task never releases an in-flight native admission or interrupts its worker.
    override fun onStopJob(params: JobParameters): Boolean {
        running.remove(params.jobId)?.stop()
        return params.jobId == SEARCH_OUTBOX_JOB_ID
    }

    private fun searchPass(): SearchOutboxWake = try {
        pollSearchOutboxes(applicationContext)
    } catch (failure: CoreException) {
        Log.i(MAIL_LOG_TAG, "Search delivery failed: ${failure.javaClass.simpleName}")
        SearchOutboxWake.Retry
    }
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
