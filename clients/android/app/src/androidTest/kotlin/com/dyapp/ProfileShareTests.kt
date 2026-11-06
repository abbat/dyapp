package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

class ProfileShareTests {
    @Test
    fun sharingOffersOnlyOpenChatsOtherThanTheSharedProfile() {
        val rows = ChatState.entries.mapIndexed { index, state ->
            ChatRow("peer-$index", state, null, 0uL, false)
        }
        val open = rows.single { it.state == ChatState.OPEN }
        assertEquals(listOf(open), profileShareTargets(rows, "another-peer"))
        assertEquals(emptyList<ChatRow>(), profileShareTargets(rows, open.peerId))
        assertEquals(emptyList<ChatRow>(), profileShareTargets(emptyList(), "another-peer"))
    }
}
