package com.dyapp

import androidx.compose.runtime.mutableStateOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.ChatSubmission

class ChatSubmissionDraftTests {
    @Test
    fun rejectedTextKeepsTheDraftAndCommittedTextClearsIt() {
        val form = ChatForm(mutableStateOf("draft"))
        form.completeSubmission(Result.failure(IllegalStateException("Queued")), editing = false)
        assertEquals("draft", form.draft.value)
        assertEquals(R.string.chat_send_failed, form.status?.text)
        for (outcome in listOf(
            ChatSubmission.Accepted(byteArrayOf(1)),
            ChatSubmission.Queued(byteArrayOf(2), "unpublished"),
        )) {
            form.draft.value = "draft"
            form.completeSubmission(Result.success(outcome), editing = false)
            assertEquals("", form.draft.value)
        }
    }

    @Test
    fun rejectedEditPreservesBothDraftsAndCommittedEditRestoresTheHeldText() {
        val form = ChatForm(mutableStateOf("edited text"))
        form.editing.value = "original target"
        form.held.value = "held draft"
        form.completeSubmission(Result.failure(IllegalStateException("disk")), editing = true)
        assertEquals("edited text", form.draft.value)
        assertEquals("held draft", form.held.value)
        assertEquals("original target", form.editing.value)
        form.completeSubmission(Result.success(ChatSubmission.Queued(byteArrayOf(1), "disk")), editing = true)
        assertEquals("held draft", form.draft.value)
        assertEquals("", form.held.value)
        assertNull(form.editing.value)
        assertEquals(R.string.chat_edit_queued, form.status?.text)
        assertNull(form.status?.details)
    }
}
