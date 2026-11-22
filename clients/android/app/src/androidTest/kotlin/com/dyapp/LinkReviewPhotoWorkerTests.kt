package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.Rgba

class LinkReviewPhotoWorkerTests {
    @Test
    fun cancellingTheUiWaiterCannotCloseAnAdmittedThumbnailRead() = runBlocking {
        withInstalledLinkReceiver { receiver ->
            val photo = GatedLinkPhoto()
            val opening = prepare(receiver, photo)
            val read = checkNotNull(opening.thumbnail(LinkReviewSource.LOCAL, 0u))
            try {
                withTimeout(5_000) { photo.entered.await() }
                cancelUiWaiter(read)
                receiver.retire()
                assertRetained(receiver, opening)
            } finally {
                photo.release.countDown()
                awaitLinkReceiverIdle(receiver)
            }
            assertArrayEquals(photo.pixels, withTimeout(5_000) { read.await() }.getOrThrow().pixels)
            assertNull(receiver.state.value.enrollment.operation)
            assertTrue(receiver.close())
            assertEquals(1, photo.closes.get())
            assertNull(opening.thumbnail(LinkReviewSource.LOCAL, 0u))
        }
    }

    private suspend fun cancelUiWaiter(read: Deferred<Result<Rgba>>) = coroutineScope {
        val waiting = CompletableDeferred<Unit>()
        val waiter = launch {
            waiting.complete(Unit)
            read.await()
        }
        withTimeout(5_000) { waiting.await() }
        waiter.cancelAndJoin()
    }

    private suspend fun prepare(receiver: LinkReceiverSession, photo: GatedLinkPhoto): LinkReviewOpening {
        val initial = LinkEnrollmentReview(
            photo.snapshot,
            { throw LinkReviewException.ReviewRequired() },
            { error("Unexpected confirmation") },
            { photo.snapshot.remote },
            { _, _ -> error("Unexpected thumbnail") },
            {}
        )
        assertTrue(receiver.finishEnrollment { initial })
        awaitLinkReceiverIdle(receiver)
        assertTrue(receiver.prepareEnrollmentReview { photo.review })
        awaitLinkReceiverIdle(receiver)
        return checkNotNull(receiver.state.value.enrollment.comparison)
    }

    private suspend fun assertRetained(receiver: LinkReceiverSession, opening: LinkReviewOpening) {
        assertTrue(receiver.state.value.busy)
        assertEquals(LinkEnrollmentOperation.PHOTO, receiver.state.value.enrollment.operation)
        assertFalse(receiver.close())
        assertNull(opening.thumbnail(LinkReviewSource.LOCAL, 0u))
    }
}
