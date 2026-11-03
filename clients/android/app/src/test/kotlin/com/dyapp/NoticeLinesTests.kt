package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

class NoticeLinesTests {
    private val peer = "12D3KooWpeer000001"

    private fun row(state: ChatState, unread: ULong, text: String = "hi", muted: Boolean = false) = ChatRow(
        peerId = peer,
        state = state,
        lastMessage = ChatMessage(
            groupId = ByteArray(0), author = peer, id = ByteArray(16), text = text, edited = false,
            deleted = false, unread = unread > 0uL, forwarded = false, delivery = null, sentAtMs = null,
        ),
        unreadCount = unread,
        muted = muted,
    )

    private fun lines(notice: ChatNotice, after: ChatRow) =
        noticeLines(notice, listOf(row(after.state, 0uL)), listOf(after))

    @Test
    fun theDefaultAddsNothing() = assertNull(lines(ChatNotice.FIXED, row(ChatState.OPEN, 1uL)))

    @Test
    fun theSettingAddsTheShortPeerIdAndTheText() {
        assertEquals(NoticeLines(shortPeerId(peer), null), lines(ChatNotice.NAME, row(ChatState.OPEN, 1uL)))
        assertEquals(NoticeLines(shortPeerId(peer), "hi"), lines(ChatNotice.TEXT, row(ChatState.OPEN, 1uL)))
    }

    @Test
    fun aRequestAMutedPeerOrNoNewUnreadStaysFixed() {
        assertNull(lines(ChatNotice.TEXT, row(ChatState.REQUEST_RECEIVED, 1uL)))
        assertNull(lines(ChatNotice.TEXT, row(ChatState.OPEN, 1uL, muted = true)))
        assertNull(noticeLines(ChatNotice.TEXT, listOf(row(ChatState.OPEN, 1uL)), listOf(row(ChatState.OPEN, 1uL))))
    }

    @Test
    fun theTextIsOneBoundedLine() {
        // A surrogate pair per code point, so a cut by UTF-16 units would split one.
        val long = "a\nb" + "\uD83D\uDE00".repeat(300)
        val text = checkNotNull(lines(ChatNotice.TEXT, row(ChatState.OPEN, 1uL, long))?.text)
        assertEquals("a b", text.take(3))
        assertEquals(200L, text.codePoints().count())
    }
}
