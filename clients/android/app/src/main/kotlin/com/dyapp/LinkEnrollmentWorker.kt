package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileSlotSelection
import uniffi.dyapp.Rgba

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
        current.value = before.copy(
            busy = true, enrollment = before.enrollment.copy(failure = null, operation = LinkEnrollmentOperation.FINISH)
        )
        worker.launch {
            val outcome = runCatching {
                val review = adoption ?: create(receipt.selection).also { adoption = it }
                // A normal false result confirms no pending link remains, without a new attempt.
                review.resume()
            }
            finished(outcome)
        }
        true
    }

    fun prepare(create: (MobileSlotSelection) -> LinkEnrollmentReview): Boolean = synchronized(monitor) {
        val before = current.value
        val receipt = before.receipt as? MobileLinkReceipt.Installed ?: return@synchronized false
        if (before.busy || before.closed || !before.enrollment.canPrepare) return@synchronized false
        current.value = before.copy(
            busy = true,
            enrollment = before.enrollment.copy(
                offered = true, failure = null, operation = LinkEnrollmentOperation.PREPARE
            )
        )
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
                        comparison = adoption?.let(::opening), failure = outcome.exceptionOrNull(), offered = true
                    ),
                    busy = false
                )
            }
        }
        true
    }

    fun confirm(expected: LinkReviewOpening, choices: List<LinkReviewChoice>): Boolean = synchronized(monitor) {
        val before = current.value
        val review = adoption ?: return@synchronized false
        if (before.busy || before.closed || !before.enrollment.canConfirm) return@synchronized false
        if (before.enrollment.comparison !== expected) return@synchronized false
        if (review.snapshot !== expected.snapshot) return@synchronized false
        val submitted = choices.map { LinkReviewChoice(it.field, it.source) }
        current.value = before.copy(
            busy = true, enrollment = before.enrollment.copy(operation = LinkEnrollmentOperation.CONFIRM)
        )
        worker.launch { confirmed(runCatching { review.confirm(submitted) }) }
        true
    }

    private fun confirmed(outcome: Result<LinkReviewSaved>) = synchronized(monitor) {
        current.value = current.value.copy(
            enrollment = current.value.enrollment.copy(
                saved = outcome.getOrNull(), failure = outcome.exceptionOrNull(), operation = null
            ),
            busy = false
        )
    }

    private fun opening(review: LinkEnrollmentReview): LinkReviewOpening = LinkReviewOpening(
        review.snapshot,
        render = { owner, choices ->
            // Pure native computation; this monitor excludes review replacement and native writes.
            synchronized(monitor) {
                val state = current.value
                check(!state.busy && !state.closed && state.enrollment.canConfirm)
                check(state.enrollment.comparison === owner && adoption === review)
                review.preview(choices.map { LinkReviewChoice(it.field, it.source) })
            }
        },
        load = { owner, source, index -> thumbnail(owner, review, source, index) }
    )

    private fun finished(outcome: Result<Boolean>) = synchronized(monitor) {
        current.value = current.value.copy(
            enrollment = current.value.enrollment.copy(
                completed = outcome.isSuccess, comparison = adoption?.let(::opening),
                failure = outcome.exceptionOrNull(), operation = null
            ),
            busy = false
        )
    }

    private fun thumbnail(
        owner: LinkReviewOpening,
        review: LinkEnrollmentReview,
        source: LinkReviewSource,
        index: UInt
    ): Deferred<Result<Rgba>>? = synchronized(monitor) {
        val state = current.value
        if (state.busy || state.closed || !state.enrollment.canConfirm) return@synchronized null
        if (state.enrollment.comparison !== owner || adoption !== review) return@synchronized null
        val result = CompletableDeferred<Result<Rgba>>()
        current.value = state.copy(
            busy = true, enrollment = state.enrollment.copy(operation = LinkEnrollmentOperation.PHOTO)
        )
        worker.launch {
            val outcome = runCatching { review.photo(source, index) }
            synchronized(monitor) {
                current.value = current.value.copy(
                    busy = false,
                    enrollment = current.value.enrollment.copy(
                        operation = null,
                        failure = outcome.exceptionOrNull()?.takeIf { it is LinkReviewException.ReviewRequired }
                            ?: current.value.enrollment.failure
                    )
                )
            }
            result.complete(outcome)
        }
        result
    }

    /** The receiver checks busy and moves to closed under the same monitor before releasing handles. */
    fun close() {
        adoption?.close()
        adoption = null
    }
}
