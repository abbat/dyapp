package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatState
import uniffi.dyapp.ContactNameView
import java.text.DateFormat
import java.util.Date

class ChatListPreviewTests {
    @get:Rule
    val compose = createComposeRule()

    private val preview = mutableStateOf(ChatListPreview(ChatPreviewBody.Empty, false, null))
    private val opened = mutableListOf<String>()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private fun string(id: Int, vararg values: Any) = context.getString(id, *values)

    private fun cases() = listOf(
        ChatPreviewBody.Empty to string(R.string.chats_no_messages),
        ChatPreviewBody.Deleted to string(R.string.chats_deleted),
        ChatPreviewBody.Text("core-bounded \u2026") to "core-bounded \u2026",
        ChatPreviewBody.Photo to string(R.string.chat_photo),
        ChatPreviewBody.Sticker("\u2605", "Walks") to string(R.string.chat_sticker_fallback, "\u2605", "Walks"),
        ChatPreviewBody.Profile("target") to string(R.string.shared_profile_card, "Robin"),
    )

    private fun show() = compose.setContent {
        MaterialTheme {
            Conversations(
                listOf(previewRow(preview.value)),
                mapOf("target" to ContactNameView("target", "Robin", null, byteArrayOf())),
                { opened += it }
            )
        }
    }

    @Test
    fun allTypedBodiesAndForwardingUseTheCorePayloadAndCachedTargetName() {
        show()
        for ((body, text) in cases()) {
            for (forwarded in listOf(false, true)) {
                compose.runOnIdle { preview.value = ChatListPreview(body, forwarded, null) }
                val expected = if (forwarded) string(R.string.chats_forwarded, text) else text
                compose.onNodeWithTag("chat-preview", useUnmergedTree = true).assertTextEquals(expected)
                compose.onNodeWithText("OLDER_UNBOUNDED_LAST_MESSAGE").assertDoesNotExist()
                compose.onNodeWithTag("chat-preview-time", useUnmergedTree = true).assertDoesNotExist()
            }
        }
        compose.onNodeWithTag("chat-1").performClick()
        compose.runOnIdle { assertEquals(listOf("conversation"), opened) }
    }

    @Test
    fun originalTimeComesOnlyFromTheCorePreviewAndLegacyTimeDoesNotFillAnAbsence() {
        val sentAt = 86_400_000uL
        preview.value = ChatListPreview(ChatPreviewBody.Photo, false, sentAt)
        show()
        val expected = DateFormat.getDateTimeInstance(
            DateFormat.SHORT,
            DateFormat.SHORT,
            context.resources.configuration.locales[0]
        ).format(Date(sentAt.toLong()))
        compose.onNodeWithTag("chat-preview-time", useUnmergedTree = true).assertTextEquals(expected)
        compose.runOnIdle { preview.value = ChatListPreview(ChatPreviewBody.Photo, false, null) }
        compose.onNodeWithTag("chat-preview-time", useUnmergedTree = true).assertDoesNotExist()
    }

    @Test
    fun actualConversationKeepsFullMessageTextInsteadOfTheBoundedListPayload() {
        val full = "full history text ".repeat(100)
        val form = ChatForm(mutableStateOf(""))
        compose.setContent {
            MaterialTheme {
                ChatContent(
                    "conversation",
                    "me",
                    ChatHistory(ChatState.OPEN, listOf(message("conversation", full)), false),
                    form,
                    ChatActions({}, {}, {}, {}, {})
                )
            }
        }
        compose.awaitNode(hasText(full)).assertTextEquals(full)
    }
}
