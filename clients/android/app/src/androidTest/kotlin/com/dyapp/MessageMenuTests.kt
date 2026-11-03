package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState

class MessageMenuTests {
    @get:Rule
    val compose = createComposeRule()

    private val form = ChatForm(mutableStateOf(""))
    private var sends = 0
    private var deletes = 0
    private val own = message("me", "one")

    private fun show(state: ChatState = ChatState.OPEN) = compose.setContent {
        val messages = listOf(own, message("peer-abcdef", "two").copy(id = byteArrayOf(2)),
            message("me", "gone").copy(id = byteArrayOf(3), deleted = true))
        val actions = ChatActions({ sends += 1 }, {}, {}, {}, {}, { deletes += 1 })
        ChatContent("peer-abcdef", "me", ChatHistory(state, messages), form, actions)
    }

    private fun press(text: String) = compose.onNodeWithText(text).performTouchInput { longClick() }

    @Test
    fun onlyAnOwnLiveTextHasTheMenu() {
        show()
        press("two")
        compose.onNodeWithTag("msg-menu").assertDoesNotExist()
        press(string(R.string.chats_deleted))
        compose.onNodeWithTag("msg-menu").assertDoesNotExist()
        press("one")
        compose.onNodeWithTag("msg-edit").assertExists()
        compose.onNodeWithTag("msg-delete").assertExists()
    }

    @Test
    fun anEditGoesThroughTheComposerAndCancelGivesTheDraftBack() {
        form.draft.value = "typed"
        show()
        press("one")
        compose.onNodeWithTag("msg-edit").performClick()
        compose.onNodeWithTag("chat-editing").assertExists()
        compose.runOnIdle { assertEquals("one", form.draft.value) }
        compose.onNodeWithTag("chat-send").assertIsNotEnabled()
        compose.runOnIdle { form.draft.value = "one, edited" }
        compose.onNodeWithTag("chat-send").assertIsEnabled().performClick()
        compose.runOnIdle { assertEquals(1, sends) }
        compose.onNodeWithTag("chat-edit-cancel").performClick()
        compose.onNodeWithTag("chat-editing").assertDoesNotExist()
        compose.runOnIdle { assertEquals("typed", form.draft.value); assertNull(form.editing.value) }
    }

    @Test
    fun deleteAsksFirst() {
        show()
        press("one")
        compose.onNodeWithTag("msg-delete").performClick()
        compose.onNodeWithTag("delete-dialog").assertExists()
        compose.onNodeWithTag("delete-cancel").performClick()
        compose.onNodeWithTag("delete-dialog").assertDoesNotExist()
        press("one")
        compose.onNodeWithTag("msg-delete").performClick()
        compose.onNodeWithTag("delete-confirm").performClick()
        compose.runOnIdle { assertEquals(1, deletes); assertNull(form.deleting.value) }
    }

    @Test
    fun aClosedChatOffersOnlyDelete() {
        show(ChatState.CLOSED)
        press("one")
        compose.onNodeWithTag("msg-edit").assertDoesNotExist()
        compose.onNodeWithTag("msg-delete").assertExists()
    }

    private fun string(id: Int) =
        InstrumentationRegistry.getInstrumentation().targetContext.getString(id)
}
