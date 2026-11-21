package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicReference

class ChatSelectionRestorationTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun sameOpeningRestoresCompleteSelectionsWithoutChoosingTheOtherAuthor() {
        val own = message("owner", "own")
        val peer = own.copy(author = "peer", text = "collision")
        val owner = Opened.Ready(Slot("selection", File("build/unopened-selection"), SlotState.READY), "owner")
        val shown = AtomicReference<ChatForm>()
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            val form = rememberOwnedChatForm(owner, "peer")
            SideEffect { shown.set(form) }
            Text(form.editing.value.orEmpty())
        }
        val previous = compose.runOnIdle {
            shown.get().apply {
                edit(own)
                deleting.value = messageKey(own)
            }
        }
        restoration.emulateSavedInstanceStateRestore()
        compose.runOnIdle {
            val restored = shown.get()
            assertNotSame(previous, restored)
            assertEquals(messageKey(own), restored.editing.value)
            assertEquals(messageKey(own), restored.deleting.value)
            assertSame(own, listOf(peer, own).byKey(restored.editing.value))
            assertSame(own, listOf(peer, own).byKey(restored.deleting.value))
        }
    }
}
