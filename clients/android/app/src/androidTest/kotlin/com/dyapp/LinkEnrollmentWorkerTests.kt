package com.dyapp

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.LinkReviewSource
import java.util.concurrent.atomic.AtomicInteger

class LinkEnrollmentWorkerTests {
    @Test
    fun exactComparisonAndFrozenChoicesSurviveRetirementBeforeSeparateCompletion() = runBlocking {
        withReceiver { receiver ->
            val snapshot = linkReviewFixture()
            val closed = AtomicInteger()
            val first = review(snapshot, { throw LinkReviewException.ReviewRequired() }) { closed.incrementAndGet() }
            assertTrue(receiver.finishEnrollment { first })
            idle(receiver)
            val pending = GatedLinkReview(snapshot.copy())
            val next = pending.review
            assertTrue(receiver.prepareEnrollmentReview { next })
            idle(receiver)
            val opening = checkNotNull(receiver.state.value.enrollment.comparison)
            val other = LinkReviewOpening(next.snapshot, { _, choices -> next.preview(choices) }, { _, _, _ -> null })
            assertNull(receiver.confirmEnrollmentReview(other, choices()))
            assertSame(next.snapshot.remote, opening.preview(choices()))
            confirmAcrossRetirement(receiver, pending)
            assertFalse(receiver.state.value.enrollment.completed)
            assertTrue(receiver.state.value.enrollment.canFinish)
            assertTrue(receiver.finishEnrollment { error("Saved review was replaced") })
            idle(receiver)
            assertTrue(receiver.state.value.enrollment.completed)
            assertTrue(receiver.close())
            assertEquals(1, closed.get())
            assertEquals(1, pending.closes.get())
        }
    }

    @Test
    fun indeterminateConfirmationCannotBeRepeatedWithoutExplicitFreshComparison() = runBlocking {
        withReceiver { receiver ->
            val snapshot = linkReviewFixture()
            val writes = AtomicInteger()
            val original = indeterminateReview(snapshot, writes)
            assertTrue(receiver.finishEnrollment { original })
            idle(receiver)
            val compared = indeterminateReview(snapshot.copy(), writes)
            assertTrue(receiver.prepareEnrollmentReview { compared })
            idle(receiver)
            val oldOpening = checkNotNull(receiver.state.value.enrollment.comparison)
            assertNotNull(receiver.confirmEnrollmentReview(oldOpening, choices()))
            idle(receiver)
            assertNull(receiver.confirmEnrollmentReview(oldOpening, choices()))
            assertFalse(receiver.finishEnrollment { error("Unknown write outcome retried") })
            assertTrue(receiver.state.value.enrollment.canPrepare)
            assertEquals(1, writes.get())
            val refreshed = review(snapshot.copy(), { false }) {}
            assertTrue(receiver.prepareEnrollmentReview { refreshed })
            idle(receiver)
            val fresh = checkNotNull(receiver.state.value.enrollment.comparison)
            assertSame(refreshed.snapshot, fresh.snapshot)
            assertSame(refreshed.snapshot.remote, fresh.preview(choices()))
            assertThrows(IllegalStateException::class.java) { oldOpening.preview(choices()) }
            assertNull(oldOpening.thumbnail(LinkReviewSource.LOCAL, 0u))
            assertNull(receiver.confirmEnrollmentReview(oldOpening, choices()))
        }
    }

    private suspend fun confirmAcrossRetirement(receiver: LinkReceiverSession, pending: GatedLinkReview) {
        val selected = choices().toMutableList()
        val opening = checkNotNull(receiver.state.value.enrollment.comparison)
        assertNotNull(receiver.confirmEnrollmentReview(opening, selected))
        try {
            withTimeout(5_000) { pending.entered.await() }
            assertThrows(IllegalStateException::class.java) { opening.preview(choices()) }
            selected.clear()
            receiver.retire()
            assertFalse(receiver.close())
            assertNull(receiver.confirmEnrollmentReview(opening, choices()))
        } finally {
            pending.release.countDown()
            idle(receiver)
        }
        assertEquals(choices(), pending.submitted.get())
    }

    private fun indeterminateReview(snapshot: LinkReviewSnapshot, writes: AtomicInteger) = LinkEnrollmentReview(
        snapshot,
        { throw LinkReviewException.ReviewRequired() },
        {
            writes.incrementAndGet()
            throw LinkReviewException.Failed("after possible save")
        },
        { snapshot.remote },
        { _, _ -> error("Unexpected thumbnail read") },
        {}
    )

    private fun choices() = listOf(LinkReviewChoice(LinkReviewField.NAME, LinkReviewSource.LOCAL))

    private fun review(
        snapshot: LinkReviewSnapshot,
        resume: () -> Boolean,
        close: () -> Unit
    ) = LinkEnrollmentReview(
        snapshot,
        resume,
        { error("Unexpected confirmation") },
        { snapshot.remote },
        { _, _ -> error("Unexpected thumbnail read") },
        close
    )

    private suspend fun idle(receiver: LinkReceiverSession) = awaitLinkReceiverIdle(receiver)

    private suspend fun withReceiver(block: suspend (LinkReceiverSession) -> Unit) = withInstalledLinkReceiver(block)
}
