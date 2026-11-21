package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileSlotSelection

/** Observed native state; retirement is presentation ownership, never a rollback claim. */
internal data class LinkReceiverSnapshot(
    val display: LinkReceiverDisplay? = null,
    val phase: MobileLinkPhase? = null,
    val receipt: MobileLinkReceipt? = null,
    val error: Throwable? = null,
    val busy: Boolean = false,
    val retired: Boolean = false,
    val closed: Boolean = false,
    val enrollment: LinkEnrollmentSnapshot = LinkEnrollmentSnapshot()
) {
    val unavailable get() = busy || retired || closed
}

/** Owns blocking work independently of a screen scope; an explicit disposition releases the handle. */
internal class LinkReceiverSession(private val create: () -> LinkReceiverCalls) {
    private val monitor = Any()
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val current = MutableStateFlow(LinkReceiverSnapshot())
    val state = current.asStateFlow()
    private var calls: LinkReceiverCalls? = null
    private var started = false
    private val enrollment = LinkEnrollmentWorker(current, monitor, worker)

    fun start(): Boolean = synchronized(monitor) {
        if (started || current.value.closed || current.value.retired) return@synchronized false
        started = true
        current.value = current.value.copy(busy = true)
        worker.launch {
            runCatching {
                val created = create()
                synchronized(monitor) {
                    calls = created
                    val phase = if (current.value.retired) created.cancel() else created.phase()
                    current.value = current.value.copy(display = created.display, phase = phase)
                }
            }.onFailure { failure ->
                synchronized(monitor) { current.value = current.value.copy(error = failure) }
            }
            synchronized(monitor) { current.value = current.value.copy(busy = false) }
        }
        true
    }

    /** One poll at a time; retiring the screen cannot cancel its actual native completion. */
    fun poll(): Boolean = synchronized(monitor) {
        val before = current.value
        val receiver = calls ?: return@synchronized false
        if (before.unavailable || before.phase != MobileLinkPhase.WAITING) return@synchronized false
        current.value = before.copy(busy = true, error = null)
        worker.launch {
            val outcome = runCatching { receiver.poll() }
            completed(receiver, outcome.exceptionOrNull())
        }
        true
    }

    private fun completed(receiver: LinkReceiverCalls, failure: Throwable?) = synchronized(monitor) {
        val phase = runCatching { receiver.phase() }
        val receipt = runCatching { receiver.receipt() }
        current.value = current.value.copy(
            phase = phase.getOrNull() ?: current.value.phase,
            receipt = receipt.getOrNull() ?: current.value.receipt,
            error = failure ?: phase.exceptionOrNull() ?: receipt.exceptionOrNull(),
            busy = false
        )
    }

    /** Explicit continuation retains the original installed destination across UI retirement. */
    fun finishEnrollment(create: (MobileSlotSelection) -> LinkEnrollmentReview): Boolean = enrollment.finish(create)

    fun prepareEnrollmentReview(create: (MobileSlotSelection) -> LinkEnrollmentReview): Boolean = enrollment.prepare(
        create
    )

    fun confirmEnrollmentReview(
        expected: LinkReviewOpening,
        choices: List<LinkReviewChoice>
    ): Deferred<Result<LinkReviewSaved>>? =
        enrollment.confirm(expected, choices)

    /** Observe non-blocking progress without reading a receipt locked by an active writer. */
    fun refreshPhase() = synchronized(monitor) {
        if (current.value.closed) return@synchronized
        calls?.let { receiver ->
            val phase = runCatching { receiver.phase() }
            current.value = current.value.copy(
                phase = phase.getOrNull() ?: current.value.phase,
                error = phase.exceptionOrNull() ?: current.value.error
            )
        }
    }

    /** Stop further UI polling; cancellation still reports core's actual admission boundary. */
    fun retire() = synchronized(monitor) {
        if (current.value.closed || current.value.retired) return@synchronized
        current.value = current.value.copy(retired = true)
        calls?.let { receiver ->
            val cancelled = runCatching { receiver.cancel() }
            current.value = current.value.copy(
                phase = cancelled.getOrNull() ?: current.value.phase,
                error = cancelled.exceptionOrNull() ?: current.value.error
            )
        }
    }

    /** Explicit disposition only; refuses an in-flight factory/poll and closes exactly once on IO. */
    suspend fun close(): Boolean = withContext(NonCancellable + Dispatchers.IO) {
        val receiver = synchronized(monitor) {
            if (current.value.busy || current.value.closed) return@withContext false
            current.value = current.value.copy(closed = true, retired = true)
            calls.also { calls = null }
        }
        try {
            try {
                enrollment.close()
            } finally {
                receiver?.close?.invoke()
            }
        } finally {
            worker.cancel()
        }
        true
    }
}
