package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

class ChatsBadgeTests {
    private fun row(state: ChatState, unread: ULong) =
        ChatRow(peerId = "peer", state = state, lastMessage = null, unreadCount = unread, muted = false)

    @Test
    fun countsUnreadMessagesAndEachRequestOnce() {
        val rows = listOf(
            row(ChatState.OPEN, 2uL),
            row(ChatState.OPEN, 0uL),
            row(ChatState.REQUEST_RECEIVED, 0uL),
            row(ChatState.REQUEST_RECEIVED, 1uL),
            row(ChatState.REQUEST_RECEIVED, 3uL),
            row(ChatState.REQUEST_SENT, 0uL),
        )
        assertEquals(7uL, chatsBadge(rows))
    }

    @Test
    fun noRowsShowNoBadge() = assertEquals(0uL, chatsBadge(emptyList()))
}
