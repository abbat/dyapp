package com.dyapp

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class ChatPhotoSendTests {
    @Test
    fun refusalRetainsPreparedPhotoButQueuedSendClearsIt() = runBlocking {
        val photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2))
        val draft = ChatPhotoDraft().apply { this.photo = photo }
        draft.send { photoSendStatus(IllegalStateException("full"), false) }
        assertSame(photo, draft.photo)
        assertEquals(R.string.chat_photo_full, draft.status?.text)
        draft.send { photoSendStatus(IllegalStateException("network"), true) }
        assertNull(draft.photo)
        assertEquals(R.string.chat_queued, draft.status?.text)
        assertNull(draft.status?.details)
    }

    @Test
    fun successfulSendClearsPreparedBytes() = runBlocking {
        val draft = ChatPhotoDraft().apply { photo = PreparedPhoto(byteArrayOf(1), byteArrayOf(2)) }
        draft.send { null }
        assertNull(draft.photo)
        assertNull(draft.status)
    }
}
