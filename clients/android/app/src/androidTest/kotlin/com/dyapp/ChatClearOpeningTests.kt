package com.dyapp

import androidx.compose.runtime.mutableStateOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatClearTarget

class ChatClearOpeningTests {
    private val form = ChatForm(mutableStateOf("")).apply { dialog.value = ChatDialog.CLEAR }
    private val opening = ChatClearOpening(form)
    private val target = ChatClearTarget("owner", "peer")
    private var clears = 0
    private val clear = { clears += 1 }

    @Test
    fun unknownOrBusyTargetCannotClear() {
        opening.confirm(null, clear)
        opening.show(target)
        form.busy = true
        opening.confirm(target, clear)
        assertEquals(0, clears)
        form.busy = false
        opening.confirm(target, clear)
        opening.confirm(target, clear)
        assertEquals(1, clears)
    }

    @Test
    fun retryRetiresTheExactPreviouslyShownTarget() {
        opening.show(target)
        assertTrue(opening.retry())
        opening.confirm(target, clear)
        val replacement = ChatClearTarget("owner", "peer")
        opening.show(replacement)
        opening.confirm(target, clear)
        assertEquals(0, clears)
        opening.confirm(replacement, clear)
        assertEquals(1, clears)
    }

    @Test
    fun cancelledOpeningCannotActOnAReopenedDialog() {
        opening.show(target)
        opening.cancel()
        form.dialog.value = ChatDialog.CLEAR
        opening.confirm(target, clear)
        opening.cancel()
        assertFalse(opening.retry())
        assertEquals(ChatDialog.CLEAR, form.dialog.value)
        assertEquals(0, clears)
    }

    @Test
    fun retiredOpeningCannotActOnAnotherDialog() {
        opening.show(target)
        opening.retire()
        opening.show(ChatClearTarget("another owner", "another peer"))
        opening.confirm(target, clear)
        opening.cancel()
        assertEquals(ChatDialog.CLEAR, form.dialog.value)
        assertEquals(0, clears)
    }

    @Test
    fun differentDialogRejectsAnOtherwiseCurrentTarget() {
        opening.show(target)
        form.dialog.value = ChatDialog.BLOCK
        opening.confirm(target, clear)
        opening.cancel()
        assertFalse(opening.retry())
        assertEquals(ChatDialog.BLOCK, form.dialog.value)
        assertEquals(0, clears)
    }
}
