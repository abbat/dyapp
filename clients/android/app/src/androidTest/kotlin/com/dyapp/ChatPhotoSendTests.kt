package com.dyapp

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.dyapp.ChatSubmission

class ChatPhotoSendTests {
    @Test
    fun refusalRetainsPreparedPhotoButQueuedSendClearsIt() = runBlocking {
        val photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2), 1u, 1u)
        val draft = ChatPhotoDraft().apply { this.photo = photo }
        draft.send { Result.failure(IllegalStateException("full")) }
        assertSame(photo, draft.photo)
        assertEquals(R.string.chat_photo_full, draft.status?.text)
        draft.send { Result.success(ChatSubmission.Queued(byteArrayOf(1), "full")) }
        assertNull(draft.photo)
        assertEquals(R.string.chat_queued, draft.status?.text)
        assertNull(draft.status?.details)
    }

    @Test
    fun throwingSenderRetainsThePhotoAndCompletesBusyState() = runBlocking {
        val photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2), 1u, 1u)
        val draft = ChatPhotoDraft().apply { this.photo = photo }
        draft.send { throw IllegalStateException("upload") }
        assertSame(photo, draft.photo)
        assertEquals(R.string.chat_send_failed, draft.status?.text)
        assertFalse(draft.busy)
    }

    @Test
    fun successfulSendClearsPreparedBytes() = runBlocking {
        val draft = ChatPhotoDraft().apply { photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2), 1u, 1u) }
        var calls = 0
        val sender = { _: PreparedPhoto ->
            calls += 1
            Result.success<ChatSubmission>(ChatSubmission.Accepted(byteArrayOf(1)))
        }
        draft.send(sender)
        draft.send(sender)
        assertEquals(1, calls)
        assertNull(draft.photo)
        assertNull(draft.status)
        assertFalse(draft.busy)
    }
}
