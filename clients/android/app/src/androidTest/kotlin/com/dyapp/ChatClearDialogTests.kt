package com.dyapp

import android.content.ClipboardManager
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class ChatClearDialogTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun completePeerCanBeCopiedAndCancelDoesNotClear() {
        val fixture = ChatClearFixture(compose)
        val context = fixture.show()
        fixture.ready()
        compose.onNodeWithTag("chat-clear-name").assertTextEquals("Robin")
        compose.onNodeWithTag("chat-clear-peer").assertTextEquals(context.getString(R.string.search_peer, fixture.peer))
        compose.onNodeWithTag("copy-chat-clear-peer").performScrollTo().performClick()
        compose.runOnIdle {
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            assertEquals(fixture.peer, clipboard.primaryClip?.getItemAt(0)?.text?.toString())
        }
        compose.onNodeWithTag("chat-clear-cancel").performClick()
        compose.onNodeWithTag("chat-clear-dialog").assertDoesNotExist()
        assertEquals(0, fixture.clears.get())
    }

    @Test
    fun failedReadHasNoDestructiveAuthorityAndRetryRequiresNewReadiness() {
        val fixture = ChatClearFixture(compose)
        fixture.fail.set(true)
        fixture.show()
        compose.awaitNode(hasTestTag("chat-clear-read-failed"))
        val old = compose.onNodeWithTag("chat-clear-confirm").assertIsNotEnabled()
            .fetchSemanticsNode()
            .config[applicationClickKey]
        compose.runOnIdle { old.invoke() }
        assertEquals(0, fixture.clears.get())
        fixture.fail.set(false)
        compose.onNodeWithTag("chat-clear-retry").performScrollTo().performClick()
        fixture.ready()
        compose.runOnIdle { old.invoke() }
        assertEquals(0, fixture.clears.get())
        fixture.ready().performClick()
        assertEquals(1, fixture.clears.get())
    }

    @Test
    fun replacingOwnedReaderRetiresInstalledConfirmAndCancelCallbacks() {
        val fixture = ChatClearFixture(compose)
        fixture.show()
        val confirm = fixture.ready().fetchSemanticsNode().config[applicationClickKey]
        val cancel = compose.onNodeWithTag("chat-clear-cancel").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { fixture.calls.value = fixture.reader() }
        fixture.ready()
        compose.runOnIdle {
            confirm.invoke()
            cancel.invoke()
            assertEquals(ChatDialog.CLEAR, fixture.form.dialog.value)
        }
        assertEquals(0, fixture.clears.get())
        fixture.ready().performClick()
        assertEquals(1, fixture.clears.get())
    }
}
