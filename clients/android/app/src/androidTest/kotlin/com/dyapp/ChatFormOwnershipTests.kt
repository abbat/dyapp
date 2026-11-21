package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.v2.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicReference

class ChatFormOwnershipTests {
    @get:Rule
    val compose = createComposeRule()
    private val opening = mutableStateOf(
        Opened.Ready(Slot("same-slot", File("build/unopened-chat-form"), SlotState.READY), "same owner")
    )
    private val peer = mutableStateOf("a".repeat(64))
    private val shown = AtomicReference<ChatForm>()

    private fun show() = compose.setContent {
        val form = rememberOwnedChatForm(opening.value, peer.value)
        SideEffect { shown.set(form) }
        Text(form.draft.value)
    }

    private fun edit(): ChatForm = compose.runOnIdle {
        shown.get().apply {
            draft.value = "old draft"
            dialog.value = ChatDialog.CLEAR
            editing.value = "old edit"
            deleting.value = "old deletion"
            held.value = "held draft"
            renaming.value = "private name"
            busy = true
            sending = true
            cleared = true
        }
    }

    private fun requireFresh(previous: ChatForm) = compose.runOnIdle {
        val current = shown.get()
        assertNotSame(previous, current)
        assertEquals("", current.draft.value)
        assertEquals("", current.held.value)
        assertNull(current.dialog.value)
        assertNull(current.editing.value)
        assertNull(current.deleting.value)
        assertNull(current.renaming.value)
        assertFalse(current.busy)
        assertFalse(current.sending)
        assertFalse(current.cleared)
    }

    @Test
    fun anotherConversationCannotInheritTheOldDraftOrConfirmation() {
        show()
        val previous = edit()
        compose.runOnIdle { peer.value = "b".repeat(64) }
        compose.waitForIdle()
        requireFresh(previous)
    }

    @Test
    fun reopeningTheSameSlotAndPeerStillRetiresTheOldForm() {
        show()
        val previous = edit()
        compose.runOnIdle { opening.value = Opened.Ready(opening.value.slot, opening.value.peerId) }
        compose.waitForIdle()
        requireFresh(previous)
    }
}
