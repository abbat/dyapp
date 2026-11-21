package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class ContactNoteOwnerTests {
    @get:Rule
    val compose = createComposeRule()

    private fun ready() = compose.awaitNode(hasTestTag("view-note-text") and isEnabled())

    private fun assertDraft(expected: String) = assertEquals(
        expected,
        compose.onNodeWithTag("view-note-text").fetchSemanticsNode().config[SemanticsProperties.EditableText].text
    )

    private fun save() {
        compose.onNodeWithTag("view-note-save").performClick()
        ready()
    }

    private fun status(resource: Int) {
        val text = InstrumentationRegistry.getInstrumentation().targetContext.getString(resource)
        compose.onNodeWithTag("view-note-status").assertTextEquals(text)
    }

    @Test
    fun aFailedSaveKeepsExactDraftForRetryAndClear() {
        val stored = AtomicReference("old")
        val writes = ConcurrentLinkedQueue<String>()
        val calls = ContactNoteCalls(read = { stored.get() }, write = {
            writes.add(it)
            check(writes.size > 1) { "controlled save failure" }
            stored.set(it)
        })
        compose.setContent { MaterialTheme { ContactNote("owner", calls) } }
        ready()
        val typed = "  \u0414\u0440\u0443\u0433 \uD83D\uDC4B\nSecond line  "
        compose.onNodeWithTag("view-note-text").performTextReplacement(typed)
        save()
        assertDraft(typed)
        status(R.string.notes_save_failed)
        save()
        assertDraft(typed)
        status(R.string.notes_saved)
        compose.onNodeWithTag("view-note-clear").performClick()
        ready()
        assertDraft("")
        status(R.string.notes_cleared)
        assertEquals(listOf(typed, typed, ""), writes.toList())
    }

    @Test
    fun unknownNoteCannotBeEditedOrSavedBeforeSuccessfulReadRetry() {
        val reads = AtomicInteger()
        val writes = AtomicInteger()
        val calls = ContactNoteCalls(read = {
            check(reads.incrementAndGet() > 1) { "controlled load failure" }
            ""
        }, write = { writes.incrementAndGet() })
        compose.setContent { MaterialTheme { ContactNote("owner", calls) } }
        compose.awaitNode(hasTestTag("view-note-retry") and isEnabled())
        compose.onNodeWithTag("view-note-text").assertIsNotEnabled()
        compose.onNodeWithTag("view-note-save").assertDoesNotExist()
        status(R.string.notes_load_failed)
        compose.onNodeWithTag("view-note-retry").performClick()
        ready()
        assertDraft("")
        compose.onNodeWithTag("view-note-save").assertIsNotEnabled()
        assertEquals(2, reads.get())
        assertEquals(0, writes.get())
    }

    @Test
    fun aLateNativeCompletionCannotReplaceAnotherOwnersDraft() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        var owner by mutableStateOf("first")
        val first = ContactNoteCalls(read = { "First note" }, write = {
            started.complete(Unit)
            try { withContext(NonCancellable) { release.await() } } finally { returned.complete(Unit) }
        })
        val second = ContactNoteCalls(read = { "Second note" }, write = {})
        compose.setContent { MaterialTheme { ContactNote(owner, if (owner == "first") first else second) } }
        ready()
        compose.onNodeWithTag("view-note-text").performTextReplacement("First private draft")
        compose.onNodeWithTag("view-note-save").performClick()
        compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
        compose.onNodeWithTag("view-note-text").assertIsNotEnabled()
        compose.runOnIdle { owner = "second" }
        ready()
        assertDraft("Second note")
        release.complete(Unit)
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        compose.waitForIdle()
        assertDraft("Second note")
        compose.onNodeWithTag("view-note-save").assertIsNotEnabled()
    }
}
