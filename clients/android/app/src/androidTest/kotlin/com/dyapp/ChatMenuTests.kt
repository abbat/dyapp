package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

class ChatMenuTests {
    @get:Rule
    val compose = createComposeRule()

    private val form = ChatForm(mutableStateOf(""))
    private var clears = 0
    private var blocks = 0

    private var mutes = 0

    private fun show(state: ChatState, messages: List<ChatMessage> = emptyList(), muted: Boolean = false) =
        compose.setContent {
            val actions = ChatActions({}, {}, {}, { clears += 1 }, { blocks += 1 }, onMute = { mutes += 1 })
            ChatContent("peer-abcdef", "me", ChatHistory(state, messages, muted), form, actions)
        }

    @Test
    fun muteNamesWhatItWillDoAndNeedsAChat() {
        show(ChatState.OPEN, muted = true)
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-mute").assertTextEquals(
            InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.chat_unmute)).performClick()
        compose.runOnIdle { assertEquals(1, mutes) }
    }

    @Test
    fun aPeerWithNoChatHasNoMute() {
        show(ChatState.NONE)
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-mute").assertDoesNotExist()
    }

    @Test
    fun clearAsksFirstAndThenShowsTheNote() {
        show(ChatState.OPEN, listOf(message("me", "one")))
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-clear").performClick()
        compose.onNodeWithTag("chat-clear-dialog").assertExists()
        compose.onNodeWithTag("chat-clear-cancel").performClick()
        compose.onNodeWithTag("chat-clear-dialog").assertDoesNotExist()
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-clear").performClick()
        compose.onNodeWithTag("chat-clear-confirm").performClick()
        compose.runOnIdle { assertEquals(1, clears); form.cleared = true }
        compose.onNodeWithTag("chat-cleared-note").assertExists()
    }

    @Test
    fun blockAsksFirst() {
        show(ChatState.OPEN)
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-block").performClick()
        compose.onNodeWithTag("block-dialog").assertExists()
        compose.onNodeWithTag("block-cancel").performClick()
        compose.runOnIdle { assertEquals(0, blocks) }
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-block").performClick()
        compose.onNodeWithTag("block-confirm").performClick()
        compose.runOnIdle { assertEquals(1, blocks) }
    }

    @Test
    fun aRequestOffersTheBlockOnItsNotice() {
        show(ChatState.REQUEST_RECEIVED, listOf(message("peer-abcdef", "hi there")))
        compose.onNodeWithTag("chat-more").performClick()
        compose.onNodeWithTag("chat-block").assertDoesNotExist()
        compose.onNodeWithTag("chat-clear").performClick()
        compose.onNodeWithTag("chat-clear-cancel").performClick()
        compose.onNodeWithTag("request-block", useUnmergedTree = true).performClick()
        compose.onNodeWithTag("block-confirm").performClick()
        compose.runOnIdle { assertEquals(1, blocks) }
    }

    @Test
    fun aClearOrBlockInFlightHoldsTheMenu() {
        form.busy = true
        show(ChatState.REQUEST_RECEIVED)
        compose.onNodeWithTag("chat-more").assertIsNotEnabled()
        compose.onNodeWithTag("request-block", useUnmergedTree = true).assertIsNotEnabled()
    }
}
