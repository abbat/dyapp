package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatDelivery
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

/** A text in a chat test's history. */
internal fun message(author: String, text: String, delivery: ChatDelivery? = null) = ChatMessage(
    groupId = byteArrayOf(0),
    author = author,
    id = byteArrayOf(1),
    text = text,
    edited = false,
    deleted = false,
    unread = false,
    forwarded = false,
    delivery = delivery,
    read = false,
    sentAtMs = null,
)

class ChatScreenTests {
    @get:Rule
    val compose = createComposeRule()

    private val form = ChatForm(mutableStateOf(""))
    private var sends = 0
    private var backs = 0
    private var profiles = 0

    private fun string(id: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    private fun show(state: ChatState?, messages: List<ChatMessage> = emptyList()) = compose.setContent {
        val history = state?.let { ChatHistory(it, messages, false) }
        val actions = ChatActions({ sends += 1 }, { profiles += 1 }, { backs += 1 }, {}, {})
        ChatContent("peer-abcdef", "me", history, form, actions)
    }

    @Test
    fun aNewConversationAllowsTheOneFirstMessage() {
        show(ChatState.NONE)
        compose.onNodeWithTag("chat-first-notice").assertExists()
        compose.onNodeWithText(string(R.string.chat_first_wait)).assertDoesNotExist()
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
        compose.onNodeWithTag("chat-input").performTextInput("hello")
        compose.onNodeWithTag("chat-send").assertIsEnabled().performClick()
        compose.runOnIdle { assertEquals(1, sends) }
    }

    @Test
    fun aSentRequestWaitsForTheReplyWithTheComposerClosed() {
        show(ChatState.REQUEST_SENT, listOf(message("me", "hello", ChatDelivery.PENDING)))
        compose.onNodeWithTag("chat-first-notice").assertExists()
        compose.onNodeWithText(string(R.string.chat_first_wait)).assertExists()
        compose.onNodeWithText("hello").assertExists()
        compose.onNodeWithText(string(R.string.chat_pending)).assertExists()
        compose.onNodeWithTag("chat-input").assertIsNotEnabled()
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
    }

    @Test
    fun aReceivedRequestTakesAReply() {
        show(ChatState.REQUEST_RECEIVED, listOf(message("peer-abcdef", "hi there")))
        compose.onNodeWithTag("chat-request-notice", useUnmergedTree = true).assertExists()
        compose.onNodeWithTag("chat-first-notice").assertDoesNotExist()
        compose.onNodeWithText("hi there").assertExists()
        compose.onNodeWithTag("chat-input").performTextInput("reply")
        compose.onNodeWithTag("chat-send").assertIsEnabled()
    }

    @Test
    fun anOpenChatShowsLaterTextsAndTheirDelivery() {
        show(
            ChatState.OPEN,
            listOf(
                message("me", "one", ChatDelivery.NODE_ACCEPTED),
                message("peer-abcdef", "two"),
                message("me", "three", ChatDelivery.DEVICE_RECEIVED),
                message("me", "five", ChatDelivery.DEVICE_RECEIVED).copy(read = true),
            )
        )
        compose.onNodeWithText(string(R.string.chat_read)).assertExists()
        compose.onNodeWithTag("chat-first-notice").assertDoesNotExist()
        compose.onNodeWithTag("chat-request-notice", useUnmergedTree = true).assertDoesNotExist()
        compose.onNodeWithText(string(R.string.chat_sent)).assertExists()
        compose.onNodeWithText(string(R.string.chat_delivered)).assertExists()
        compose.onNodeWithTag("chat-input").performTextInput("four")
        compose.onNodeWithTag("chat-send").assertIsEnabled()
    }

    @Test
    fun aChatStillLoadingTakesNoText() {
        show(null)
        compose.onNodeWithTag("chat-input").assertIsNotEnabled()
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
    }

    @Test
    fun aTooLongDraftCannotBeSentAndSaysWhy() {
        form.draft.value = "a".repeat(65_537)
        show(ChatState.OPEN)
        compose.onNodeWithText(string(R.string.chat_too_long)).assertExists()
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
    }

    @Test
    fun aSendInFlightKeepsItsTextAndBlocksAnother() {
        form.draft.value = "hello"
        form.sending = true
        show(ChatState.OPEN)
        compose.onNodeWithText(string(R.string.chat_sending)).assertExists()
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
        compose.onNodeWithTag("chat-input").assertIsEnabled()
    }

    @Test
    fun aFailureShowsItsReasonAndAQueuedSendSaysSo() {
        form.status = ChatStatus(R.string.chat_send_failed, "details")
        show(ChatState.OPEN)
        compose.onNodeWithText(string(R.string.chat_send_failed)).assertExists()
        compose.runOnIdle { form.status = ChatStatus(R.string.chat_queued, null) }
        compose.onNodeWithText(string(R.string.chat_queued)).assertExists()
    }

    @Test
    fun theHeaderOpensTheProfile() {
        show(ChatState.OPEN)
        compose.onNodeWithTag("chat-profile").performClick()
        compose.runOnIdle { assertEquals(1, profiles) }
    }

    @Test
    fun theBackButtonLeavesTheChat() {
        show(ChatState.OPEN)
        compose.onNodeWithTag("chat-back").performClick()
        compose.runOnIdle { assertEquals(1, backs) }
    }
}
