package com.dyapp

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatState

class ChatComposerTests {
    @Test
    fun onlyLoadingClosedAndWaitingForAReplyDisableTyping() {
        for (state in listOf(ChatState.NONE, ChatState.REQUEST_RECEIVED, ChatState.OPEN)) {
            assertTrue(chatWritable(state))
            assertTrue(chatSendable(state, "text"))
        }
        for (state in listOf(null, ChatState.REQUEST_SENT, ChatState.CLOSED)) {
            assertFalse(chatWritable(state))
            assertFalse(chatSendable(state, "text"))
        }
    }

    @Test
    fun sendabilityCountsUtf8BytesThroughTheRealNativeAdapter() {
        for ((unit, count) in listOf("a" to 65_536, "\u0416" to 32_768, "\ud83d\ude00" to 16_384)) {
            val boundary = unit.repeat(count)
            assertTrue(chatSendable(ChatState.OPEN, boundary))
            assertFalse(chatTooLong(boundary))
            assertFalse(chatSendable(ChatState.OPEN, boundary + unit))
            assertTrue(chatTooLong(boundary + unit))
        }
    }

    @Test
    fun blankTextUsesLinuxDesktopWhitespaceSemantics() {
        for (blank in listOf("", " \n", "\u00a0", "\ufeff")) {
            assertFalse(chatSendable(ChatState.OPEN, blank))
        }
        assertTrue(chatSendable(ChatState.OPEN, "\u0085"))
        assertTrue(chatSendable(ChatState.OPEN, "\u001c"))
    }
}
