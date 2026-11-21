package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class ChatConfirmOwnershipTests {
    @get:Rule
    val compose = createComposeRule()

    private val form = mutableStateOf(ChatForm(mutableStateOf("")))
    private var blocks = 0
    private val deleted = mutableListOf<String?>()

    private fun show() = compose.setContent {
        val owner = form.value
        val actions = ChatActions({}, {}, {}, {}, { blocks += 1 }, { deleted.add(owner.deleting.value) })
        MaterialTheme { ChatConfirm(owner, actions, null) }
    }

    private fun action(tag: String) =
        compose.onNodeWithTag(tag).fetchSemanticsNode().config[applicationClickKey]

    @Test
    fun cancelledBlockCannotConfirmOrCancelTheReopenedDialog() {
        form.value.dialog.value = ChatDialog.BLOCK
        show()
        val oldConfirm = action("block-confirm")
        val oldCancel = action("block-cancel")
        compose.onNodeWithTag("block-cancel").performClick()
        compose.onNodeWithTag("block-dialog").assertDoesNotExist()
        compose.runOnIdle { form.value.dialog.value = ChatDialog.BLOCK }
        val current = action("block-confirm")
        compose.runOnIdle {
            oldConfirm()
            oldCancel()
            assertEquals(ChatDialog.BLOCK, form.value.dialog.value)
            assertEquals(0, blocks)
            current()
            current()
        }
        assertEquals(1, blocks)
    }

    @Test
    fun replacingTheSelectedMessageRetiresBothInstalledDeleteCallbacks() {
        form.value.deleting.value = "first"
        show()
        val oldConfirm = action("delete-confirm")
        val oldCancel = action("delete-cancel")
        compose.runOnIdle { form.value.deleting.value = "second" }
        val current = action("delete-confirm")
        compose.runOnIdle {
            oldConfirm()
            oldCancel()
            assertEquals("second", form.value.deleting.value)
            assertEquals(emptyList<String?>(), deleted)
            current()
            current()
        }
        assertEquals(listOf("second"), deleted)
    }

    @Test
    fun cancelAndReopenOfTheSameMessageStillRequireFreshConfirmation() {
        form.value.deleting.value = "same"
        show()
        val old = action("delete-confirm")
        compose.onNodeWithTag("delete-cancel").performClick()
        compose.onNodeWithTag("delete-dialog").assertDoesNotExist()
        compose.runOnIdle { form.value.deleting.value = "same" }
        val current = action("delete-confirm")
        compose.runOnIdle {
            old()
            assertEquals(emptyList<String?>(), deleted)
            current()
        }
        assertEquals(listOf("same"), deleted)
    }

    @Test
    fun busyAdmissionIsRejectedEvenBeforeTheNextComposition() {
        form.value.dialog.value = ChatDialog.BLOCK
        show()
        val confirm = action("block-confirm")
        val cancel = action("block-cancel")
        compose.runOnIdle {
            form.value.busy = true
            confirm()
            cancel()
            assertEquals(0, blocks)
            assertEquals(ChatDialog.BLOCK, form.value.dialog.value)
        }
        compose.onNodeWithTag("block-confirm").assertIsNotEnabled()
        compose.onNodeWithTag("block-cancel").assertIsNotEnabled()
        compose.runOnIdle { form.value.busy = false }
        compose.onNodeWithTag("block-confirm").performClick()
        assertEquals(1, blocks)
    }

    @Test
    fun replacingTheOwnerRetiresAnEqualMessageSelection() {
        form.value.deleting.value = "same"
        show()
        val confirm = action("delete-confirm")
        val cancel = action("delete-cancel")
        compose.runOnIdle { form.value = ChatForm(mutableStateOf("")).apply { deleting.value = "same" } }
        val current = action("delete-confirm")
        compose.runOnIdle {
            confirm()
            cancel()
            assertEquals("same", form.value.deleting.value)
            assertEquals(emptyList<String?>(), deleted)
            current()
        }
        assertEquals(listOf("same"), deleted)
    }
}
