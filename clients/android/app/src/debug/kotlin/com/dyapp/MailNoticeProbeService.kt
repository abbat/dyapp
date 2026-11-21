package com.dyapp

import android.app.Notification
import android.app.job.JobParameters
import java.util.concurrent.ConcurrentHashMap

/** Observations only: the service still runs the real registry, native receive and OS posting. */
internal interface MailNoticeJobProbe {
    fun started()
    fun attempted(slotId: String, peerId: String, notice: Notification)
    fun stopped()
    fun finished(completionRequested: Boolean, retryRequested: Boolean)
}

internal val mailNoticeJobProbes = ConcurrentHashMap<Pair<String?, Int>, MailNoticeJobProbe>()

/** Debug-only framework endpoint; no replaceable native backend or notification implementation. */
class MailNoticeProbeService : MailJob() {
    private val probes = ConcurrentHashMap<JobParameters, MailNoticeJobProbe>()
    private val active = ConcurrentHashMap<Pair<String?, Int>, JobParameters>()

    override fun onStartJob(params: JobParameters): Boolean {
        val probe = mailNoticeJobProbes.remove(params.scopedJobId()) ?: return false
        probes[params] = probe
        active[params.scopedJobId()] = params
        probe.started()
        return super.onStartJob(params)
    }

    override fun onStopJob(params: JobParameters): Boolean {
        val retry = super.onStopJob(params)
        active.remove(params.scopedJobId())?.let { probes[it]?.stopped() }
        return retry
    }

    override fun noticeAttempted(params: JobParameters, slotId: String, peerId: String, notice: Notification) {
        probes[params]?.attempted(slotId, peerId, notice)
    }

    override fun workerFinished(params: JobParameters, completionRequested: Boolean, retryRequested: Boolean) {
        probes.remove(params)?.let { probe ->
            active.remove(params.scopedJobId(), params)
            probe.finished(completionRequested, retryRequested)
        }
    }
}
