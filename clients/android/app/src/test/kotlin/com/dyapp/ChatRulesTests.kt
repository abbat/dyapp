package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.ChatSubmission

class ChatRulesTests {
    @Test
    fun aFailedLikeSaysWhatToDoAboutIt() {
        assertEquals(R.string.view_like_unpublished, likeStatus(RuntimeException("unpublished")).text)
        assertEquals(R.string.view_like_not_connected, likeStatus(RuntimeException("notConnected")).text)
        assertEquals(R.string.view_like_failed, likeStatus(RuntimeException("disk")).text)
    }

    @Test
    fun acceptedAndQueuedAreCommittedRegardlessOfErrorDetails() {
        assertNull(sendStatus(Result.success(ChatSubmission.Accepted(byteArrayOf(1)))))
        val queued = Result.success(ChatSubmission.Queued(byteArrayOf(2), "notConnected unpublished full"))
        assertEquals(R.string.chat_queued, sendStatus(queued)?.text)
        assertNull(sendStatus(queued)?.details)
        assertEquals(R.string.chat_edit_queued, sendStatus(queued, R.string.chat_edit_queued)?.text)
        assertEquals(R.string.chat_delete_queued, sendStatus(queued, R.string.chat_delete_queued)?.text)
    }

    @Test
    fun rejectedOperationsNeverBecomeQueuedFromTheirErrorText() {
        val error = Result.failure<ChatSubmission>(IllegalStateException("notConnected"))
        assertEquals(R.string.search_not_connected, sendStatus(error)?.text)
        val unpublished = Result.failure<ChatSubmission>(IllegalStateException("unpublished"))
        assertEquals(R.string.chat_unpublished, sendStatus(unpublished)?.text)
        val failure = Result.failure<ChatSubmission>(IllegalStateException("Queued"))
        assertEquals(R.string.chat_send_failed, sendStatus(failure)?.text)
        assertEquals(R.string.chat_edit_failed, sendStatus(failure, failed = R.string.chat_edit_failed)?.text)
    }
}
