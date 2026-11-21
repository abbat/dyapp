package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatSubmission

class ChatForwardUiTests {
    @get:Rule
    val compose = createComposeRule()

    private fun ask(text: String) {
        compose.onNodeWithText(text).performTouchInput { longClick() }
        compose.onNodeWithTag("msg-forward").performClick()
    }

    @Test
    fun receivedForwardChoosesAnOffPageTargetAndQueuedCommitPreservesTheDraft() {
        val f = ChatForwardUiFixture(compose)
        f.outcome = ChatSubmission.Queued(ByteArray(16), "offline")
        f.show()
        compose.onNodeWithText("Received text").performTouchInput { longClick() }
        compose.onNodeWithTag("msg-edit").assertDoesNotExist()
        compose.onNodeWithTag("msg-delete").assertDoesNotExist()
        compose.onNodeWithTag("msg-forward").performClick()
        f.ready()
        val target = f.choices.last()
        compose.onNodeWithTag("collection-query").performScrollTo().performTextReplacement(target)
        compose.awaitNode(hasTestTag("forward-target-" + target)).performScrollTo().performClick()
        val confirm = f.ready().fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle {
            confirm()
            confirm()
        }
        compose.waitUntil(60_000) { f.navigated.isNotEmpty() }
        assertEquals(listOf(target), f.navigated.toList())
        assertEquals(1, f.sends.get())
        assertEquals(f.peer, f.sent.get().first.peerId)
        assertEquals(f.peer, f.sent.get().first.author)
        assertArrayEquals(f.received.id, f.sent.get().first.messageId)
        assertArrayEquals(f.received.groupId, f.sent.get().first.groupId)
        assertEquals("draft", f.form.draft.value)
        assertEquals("held", f.form.held.value)
        assertEquals("edit key", f.form.editing.value)
    }

    @Test
    fun ownForwardCanBeCancelledAndOldConfirmCannotActAfterReopeningTheIdentity() {
        val f = ChatForwardUiFixture(compose)
        f.show()
        ask("Own text")
        val old = f.ready().fetchSemanticsNode().config[applicationClickKey]
        compose.onNodeWithTag("chat-forward-cancel").performClick()
        compose.onNodeWithTag("chat-forward-dialog").assertDoesNotExist()
        compose.runOnIdle { f.opened.value = Opened.Ready(f.opened.value.slot, f.me) }
        compose.runOnIdle { old() }
        assertEquals(0, f.sends.get())
        ask("Own text")
        f.ready().performClick()
        compose.waitUntil(60_000) { f.navigated.isNotEmpty() }
        assertEquals(1, f.sends.get())
        assertEquals(f.me, f.sent.get().first.author)
    }

    @Test
    fun replacingTheOwnerWhilePickerIsOpenRetiresItsHeldAuthority() {
        val f = ChatForwardUiFixture(compose)
        f.show()
        ask("Own text")
        val callback = f.ready().fetchSemanticsNode().config[applicationClickKey]
        val previous = f.current.get()
        compose.runOnIdle { f.opened.value = Opened.Ready(f.opened.value.slot, f.me) }
        compose.onNodeWithTag("chat-forward-dialog").assertDoesNotExist()
        assertNull(previous.state.opening)
        compose.runOnIdle { callback() }
        assertEquals(0, f.sends.get())
    }

    @Test
    fun emptyTargetsCannotAdmitForwardingAndCancelStagesNothing() {
        val f = ChatForwardUiFixture(compose)
        f.offered.set(emptyList())
        f.show()
        ask("Own text")
        compose.awaitNode(hasTestTag("forward-no-targets"))
        compose.onNodeWithTag("chat-forward-confirm").assertIsNotEnabled()
        compose.onNodeWithTag("chat-forward-cancel").performClick()
        compose.onNodeWithTag("chat-forward-dialog").assertDoesNotExist()
        assertEquals(0, f.sends.get())
    }

    @Test
    fun failedSendRetainsDestinationUntilExplicitRetry() {
        val f = ChatForwardUiFixture(compose)
        f.failSend = true
        f.show()
        ask("Own text")
        f.ready()
        val target = f.choices[1]
        compose.onNodeWithTag("forward-target-" + target).performScrollTo().performClick()
        f.ready().performClick()
        compose.awaitNode(hasTestTag("forward-failed"))
        assertEquals(1, f.sends.get())
        assertEquals(emptyList<String>(), f.navigated.toList())
        assertEquals(target, f.current.get().state.chosen)
        assertEquals("draft", f.form.draft.value)
        compose.runOnIdle { f.failSend = false }
        f.ready().performClick()
        compose.waitUntil(60_000) { f.navigated.isNotEmpty() }
        assertEquals(2, f.sends.get())
        assertEquals(listOf(target), f.navigated.toList())
    }

    @Test
    fun failedVerificationMustBeRetriedBeforeAnyDestinationCanBeSent() {
        val f = ChatForwardUiFixture(compose)
        f.failRead.set(true)
        f.show()
        ask("Own text")
        compose.awaitNode(hasTestTag("forward-failed"))
        compose.onNodeWithTag("chat-forward-confirm").assertIsNotEnabled()
        f.failRead.set(false)
        compose.onNodeWithTag("forward-retry").performScrollTo().performClick()
        f.ready()
        assertEquals(0, f.sends.get())
        compose.onNodeWithTag("chat-forward-cancel").performClick()
    }
}
