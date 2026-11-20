package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatPhotoDownload
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class PhotoDownloadOwnerTests {
    @get:Rule
    val compose = createComposeRule()

    private fun ready(tag: String) = compose.awaitNode(hasTestTag(tag) and isEnabled())

    private fun finishNative(release: CompletableDeferred<Unit>, returned: CompletableDeferred<Unit>) {
        release.complete(Unit)
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        compose.waitForIdle()
    }

    @Test
    fun unknownChoiceRequiresReadRetryAndDoesNotWriteADefault() {
        val fail = AtomicBoolean(true)
        val writes = AtomicInteger()
        val calls = PhotoDownloadCalls(read = {
            check(!fail.get()) { "controlled read failure" }
            ChatPhotoDownload.BY_NETWORK
        }, write = { writes.incrementAndGet() })
        compose.setContent { MaterialTheme { PhotoDownloadPicker("owner", calls) } }
        ready("photo-mode-retry")
        listOf("photo-mode-network", "photo-mode-always", "photo-mode-tap").forEach {
            compose.onNodeWithTag(it).assertIsNotEnabled()
        }
        assertEquals(0, writes.get())
        fail.set(false)
        compose.onNodeWithTag("photo-mode-retry").performClick()
        ready("photo-mode-network")
        compose.onNodeWithTag("photo-mode-network").assertIsSelected()
        assertEquals(0, writes.get())
    }

    @Test
    fun failedWriteRequiresReadbackBeforeAnotherChoice() {
        val stored = AtomicReference(ChatPhotoDownload.BY_NETWORK)
        val writes = AtomicInteger()
        val calls = PhotoDownloadCalls(read = { stored.get() }, write = {
            writes.incrementAndGet()
            stored.set(it)
            error("controlled indeterminate write")
        })
        compose.setContent { MaterialTheme { PhotoDownloadPicker("owner", calls) } }
        ready("photo-mode-always")
        compose.onNodeWithTag("photo-mode-always").performClick()
        ready("photo-mode-retry")
        compose.onNodeWithTag("photo-mode-network").assertIsNotEnabled()
        compose.onNodeWithTag("photo-mode-tap").assertIsNotEnabled()
        assertEquals(1, writes.get())
        compose.onNodeWithTag("photo-mode-retry").performClick()
        assertSelected("photo-mode-always")
        assertEquals(1, writes.get())
    }

    private fun assertSelected(tag: String) {
        ready(tag)
        compose.onNodeWithTag(tag).assertIsSelected()
    }

    @Test
    fun repeatedAdmissionAndLateCompletionStayWithTheOriginalOwner() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        val stored = AtomicReference(ChatPhotoDownload.BY_NETWORK)
        val writes = AtomicInteger()
        var owner by mutableStateOf("first")
        val first = PhotoDownloadCalls(read = { stored.get() }, write = {
            writes.incrementAndGet()
            started.complete(Unit)
            try {
                // Model a native call that retains its operation through actual completion.
                withContext(NonCancellable) { release.await() }
                stored.set(it)
            } finally { returned.complete(Unit) }
        })
        val second = PhotoDownloadCalls(read = { ChatPhotoDownload.ON_TAP }, write = {})
        compose.setContent { MaterialTheme { PhotoDownloadPicker(owner, if (owner == "first") first else second) } }
        ready("photo-mode-always")
        val action = compose.onNodeWithTag("photo-mode-always").fetchSemanticsNode().config[SemanticsActions.OnClick]
        compose.runOnIdle { repeat(2) { action.action?.invoke() } }
        compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
        compose.onNodeWithTag("photo-mode-tap").assertIsNotEnabled()
        assertEquals(1, writes.get())
        compose.runOnIdle { owner = "second" }
        assertSelected("photo-mode-tap")
        finishNative(release, returned)
        compose.onNodeWithTag("photo-mode-tap").assertIsSelected()
        assertEquals(ChatPhotoDownload.ALWAYS, stored.get())
    }
}
