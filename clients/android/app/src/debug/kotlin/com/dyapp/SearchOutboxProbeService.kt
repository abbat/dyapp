package com.dyapp

import android.app.job.JobParameters
import uniffi.dyapp.SearchOutboxWake
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicReference

/** Readiness callbacks for the real framework lifecycle and actual native delivery pass. */
internal interface SearchOutboxJobProbe {
    fun poll(): SearchOutboxWake
    fun stopped()
    fun finished(completionRequested: Boolean, retryRequested: Boolean)
}

internal val searchOutboxJobProbe = AtomicReference<SearchOutboxJobProbe?>()

/** Exists only in debug: the release service has no configurable native backend. */
class SearchOutboxProbeService : MailJob() {
    private val probes = ConcurrentHashMap<JobParameters, SearchOutboxJobProbe>()
    private val active = ConcurrentHashMap<Pair<String?, Int>, JobParameters>()

    override fun onStartJob(params: JobParameters): Boolean {
        val probe = searchOutboxJobProbe.get() ?: return false
        probes[params] = probe
        active[params.scopedJobId()] = params
        return super.onStartJob(params)
    }

    override fun onStopJob(params: JobParameters): Boolean {
        val retry = super.onStopJob(params)
        active.remove(params.scopedJobId())?.let { probes[it]?.stopped() }
        return retry
    }

    override fun isSearchJob(params: JobParameters): Boolean = true

    override fun searchPass(params: JobParameters): SearchOutboxWake = checkNotNull(probes[params]).poll()

    override fun workerFinished(params: JobParameters, completionRequested: Boolean, retryRequested: Boolean) {
        probes.remove(params)?.let { probe ->
            active.remove(params.scopedJobId(), params)
            probe.finished(completionRequested, retryRequested)
        }
    }
}
