package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatLikeAttempt
import uniffi.dyapp.ChatLikeState
import uniffi.dyapp.ChatLikes
import uniffi.dyapp.ChatSubmission

class ProfileLikeOutcomeTests {
    private val liked = ChatLikes(true, false, false)
    private val unliked = ChatLikes(false, false, false)

    @Test
    fun aMatchingOldLikeDoesNotMakeARejectedAttemptPending() {
        val form = ProfileForm().apply { likes = liked }
        val request = form.start()
        form.completeLike(request, Result.failure(IllegalStateException("Queued")))
        form.finishRead(form.readVersion, Result.success(liked))
        assertEquals(liked, form.likes)
        assertEquals(R.string.view_like_failed, form.status?.text)
        assertNull(form.info)
        assertFalse(form.sending)
    }

    @Test
    fun onlyAnActualQueuedSubmissionShowsPending() {
        for (submission in listOf(
            null,
            ChatSubmission.Accepted(byteArrayOf(1)),
            ChatSubmission.Queued(byteArrayOf(2), "unpublished"),
        )) {
            val form = ProfileForm()
            val request = form.start()
            form.completeLike(request, Result.success(ChatLikeAttempt(submission, ChatLikeState.Ready(liked))))
            assertEquals(liked, form.likes)
            assertEquals(if (submission is ChatSubmission.Queued) R.string.view_like_pending else null, form.info)
            assertNull(form.status)
            assertFalse(form.sending)
        }
    }

    @Test
    fun anUnavailableProjectionKeepsCommitmentAndWaitsForARealReadCompletion() {
        val form = ProfileForm().apply { likes = unliked }
        val request = form.start()
        val queued = ChatSubmission.Queued(byteArrayOf(1), "node failure")
        form.completeLike(request, Result.success(ChatLikeAttempt(queued, ChatLikeState.Unavailable("disk"))))
        assertNull(form.likes)
        assertFalse(form.likeReady)
        assertEquals(R.string.view_like_pending, form.info)
        assertEquals(R.string.view_like_state_unavailable, form.status?.text)
        assertFalse(form.sending)
        form.finishRead(form.readVersion, Result.success(liked))
        assertEquals(liked, form.likes)
        assertTrue(form.likeReady)
        assertNull(form.status)
        assertEquals(R.string.view_like_pending, form.info)
    }

    @Test
    fun delayedReadsAndRepeatedCompletionsCannotOverwriteTheCurrentAttempt() {
        val form = ProfileForm()
        val oldRead = form.readVersion
        val delayed = { form.finishRead(oldRead, Result.success(unliked)) }
        val request = form.start()
        form.finishRead(request, Result.success(unliked))
        assertNull(form.likes)
        form.completeLike(request, Result.success(ChatLikeAttempt(null, ChatLikeState.Ready(liked))))
        delayed()
        form.completeLike(request, Result.failure(IllegalStateException("stale")))
        assertEquals(liked, form.likes)
        assertNull(form.status)
    }

    @Test
    fun aSupersededAttemptCannotUnlockTheNewerOne() {
        val form = ProfileForm()
        val old = form.start()
        val current = form.start()
        form.completeLike(old, Result.success(ChatLikeAttempt(null, ChatLikeState.Ready(unliked))))
        assertTrue(form.sending)
        assertNull(form.likes)
        form.completeLike(current, Result.success(ChatLikeAttempt(null, ChatLikeState.Ready(liked))))
        assertFalse(form.sending)
        assertEquals(liked, form.likes)
    }
}
