package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatMessage

class ChatRulesTests {
    private fun message(author: String, id: Byte) = ChatMessage(
        groupId = byteArrayOf(0),
        author = author,
        id = byteArrayOf(id),
        text = "text",
        edited = false,
        deleted = false,
        unread = false,
        forwarded = false,
        delivery = null,
        read = false,
        sentAtMs = null,
    )

    @Test
    fun aFailedLikeSaysWhatToDoAboutIt() {
        assertEquals(R.string.view_like_unpublished, likeStatus(RuntimeException("unpublished")).text)
        assertEquals(R.string.view_like_not_connected, likeStatus(RuntimeException("notConnected")).text)
        assertEquals(R.string.view_like_failed, likeStatus(RuntimeException("disk")).text)
    }

    @Test
    fun aNewOwnMessageMeansTheFailedSendWasStored() {
        val before = listOf(message("me", 1), message("peer", 2))
        assertFalse(stagedNew(before, before, "me"))
        assertFalse(stagedNew(before, before + message("peer", 3), "me"))
        assertTrue(stagedNew(before, before + message("me", 3), "me"))
    }

    @Test
    fun aStoredSendIsQueuedAndClearsTheDraftOtherFailuresKeepIt() {
        val error = IllegalStateException("notConnected")
        assertEquals(R.string.chat_queued, sendStatus(error, staged = true).text)
        assertNull(sendStatus(error, staged = true).details)
        assertEquals(R.string.search_not_connected, sendStatus(error, staged = false).text)
        val unpublished = sendStatus(IllegalStateException("unpublished"), staged = false)
        assertEquals(R.string.chat_unpublished, unpublished.text)
        assertEquals(R.string.chat_send_failed, sendStatus(IllegalStateException("x"), staged = false).text)
    }
}
