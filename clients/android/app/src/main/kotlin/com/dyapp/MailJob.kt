package com.dyapp

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.util.Log
import kotlin.concurrent.thread

internal const val MAIL_JOB_ID = 1

// The shortest period JobScheduler allows; Doze and App Standby stretch it further.
private const val MAIL_JOB_PERIOD_MS = 15 * 60_000L

/**
 * Receives the active identity's mail while the app is not on screen. The identity key is not
 * bound to a user check, so this also runs while the app lock is on; the lock only covers the UI.
 */
class MailJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        thread(name = "mail-job") {
            receiveMail(applicationContext)
            jobFinished(params, false)
        }
        return true
    }

    // A receive in Rust cannot be interrupted; the next period simply runs again.
    override fun onStopJob(params: JobParameters) = false
}

/**
 * One receive for the active ready identity, then a notice when it left more to read; none, a
 * recovery state or a failure stays quiet.
 */
internal fun receiveMail(context: Context) {
    runCatching {
        val opened = RustBridge.slots(context).open() as? Opened.Ready ?: return
        val more = RustBridge.withChats(opened) {
            val before = noticeCount(it)
            it.receive()
            noticeCount(it) > before
        }
        if (more) postMailNotice(context)
    }.onFailure { Log.i("dyapp", "Background receive failed: ${it.javaClass.simpleName}") }
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
