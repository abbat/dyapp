package com.dyapp

import androidx.compose.runtime.mutableStateOf
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatSubmission
import java.io.File
import kotlin.coroutines.CoroutineContext

class ChatSubmissionDraftTests {
    @Test
    fun missingEditTargetNeverDispatchesANewMessageOrChangesTheDrafts() {
        val form = ChatForm(mutableStateOf("edited text"))
        form.editing.value = "missing target"
        form.held.value = "held draft"
        val previousStatus = ChatStatus(R.string.chat_edit_failed, "previous failure")
        form.status = previousStatus
        val pending = ArrayDeque<Runnable>()
        val dispatcher = object : CoroutineDispatcher() {
            override fun dispatch(context: CoroutineContext, block: Runnable) {
                pending.addLast(block)
            }
        }
        val scope = CoroutineScope(Job() + dispatcher)
        // A presentation-only owner: this test must never open its native registry or storage.
        val slot = Slot("stale-edit", File("build/unopened-stale-edit"), SlotState.READY)
        val feed = ChatFeed(Opened.Ready(slot, "me"))
        try {
            scope.sendDraft(form, feed, "peer", emptyList())
            assertTrue("A missing edit target dispatched a submission", pending.isEmpty())
            assertFalse(form.sending)
            assertSame(previousStatus, form.status)
            assertEquals("edited text", form.draft.value)
            assertEquals("held draft", form.held.value)
            assertEquals("missing target", form.editing.value)
        } finally {
            scope.cancel()
            // Cancellation drains a mistakenly dispatched task without running native IO.
            while (pending.isNotEmpty()) pending.removeFirst().run()
        }
    }

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
