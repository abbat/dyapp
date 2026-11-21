package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileSlotSelection

/** Shares the receiving worker's admission and lifetime; UI scopes never own a native write. */
internal class LinkEnrollmentWorker(
    private val current: MutableStateFlow<LinkReceiverSnapshot>,
    private val monitor: Any,
    private val worker: CoroutineScope
) {
    private var adoption: LinkEnrollmentReview? = null

    fun finish(create: (MobileSlotSelection) -> LinkEnrollmentReview): Boolean = synchronized(monitor) {
        val before = current.value
        val receipt = before.receipt as? MobileLinkReceipt.Installed ?: return@synchronized false
        if (before.busy || before.closed || !before.enrollment.canFinish) return@synchronized false
        current.value = before.copy(busy = true, enrollment = before.enrollment.copy(failure = null))
        worker.launch {
            val outcome = runCatching {
                val review = adoption ?: create(receipt.selection).also { adoption = it }
                // A normal false result confirms no pending link remains, without a new attempt.
                review.resume()
            }
            synchronized(monitor) {
                current.value = current.value.copy(
                    enrollment = current.value.enrollment.copy(
                        completed = outcome.isSuccess,
                        comparison = adoption?.snapshot,
                        failure = outcome.exceptionOrNull()
                    ),
                    busy = false
                )
            }
        }
        true
    }

    fun prepare(create: (MobileSlotSelection) -> LinkEnrollmentReview): Boolean = synchronized(monitor) {
        val before = current.value
        val receipt = before.receipt as? MobileLinkReceipt.Installed ?: return@synchronized false
        if (before.busy || before.closed || !before.enrollment.canPrepare) return@synchronized false
        current.value = before.copy(busy = true, enrollment = before.enrollment.copy(offered = true, failure = null))
        worker.launch {
            val outcome = runCatching {
                val replacement = create(receipt.selection)
                val previous = adoption
                adoption = replacement
                previous?.close()
            }
            synchronized(monitor) {
                current.value = current.value.copy(
                    enrollment = LinkEnrollmentSnapshot(
                        comparison = adoption?.snapshot, failure = outcome.exceptionOrNull(), offered = true
                    ),
                    busy = false
                )
            }
        }
        true
    }

    fun confirm(expected: LinkReviewSnapshot, choices: List<LinkReviewChoice>): Boolean = synchronized(monitor) {
        val before = current.value
        val review = adoption ?: return@synchronized false
        if (before.busy || before.closed || !before.enrollment.canConfirm) return@synchronized false
        if (before.enrollment.comparison !== expected || review.snapshot !== expected) return@synchronized false
        val submitted = choices.map { LinkReviewChoice(it.field, it.source) }
        current.value = before.copy(busy = true, enrollment = before.enrollment.copy(confirmationAttempted = true))
        worker.launch { confirmed(runCatching { review.confirm(submitted) }) }
        true
    }

    private fun confirmed(outcome: Result<LinkReviewSaved>) = synchronized(monitor) {
        current.value = current.value.copy(
            enrollment = current.value.enrollment.copy(
                saved = outcome.getOrNull(), failure = outcome.exceptionOrNull()
            ),
            busy = false
        )
    }

    /** The receiver checks busy and moves to closed under the same monitor before releasing handles. */
    fun close() {
        adoption?.close()
        adoption = null
    }
}
