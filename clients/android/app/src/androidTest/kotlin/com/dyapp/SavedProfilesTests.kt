package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class SavedProfilesTests {
    @get:Rule
    val compose = createComposeRule()
    private val peer = "ab".repeat(32)
    private val expanded = mutableStateOf(true)

    private fun ready() = compose.awaitNode(hasTestTag("saved-profiles-refresh") and isEnabled())

    @Test
    fun collectionReadsLocallyAndOnlyExplicitRowOpenNavigates() {
        val reads = AtomicInteger()
        val opened = mutableListOf<String>()
        val collapsed = mutableStateOf(false)
        val calls = SavedProfilesCalls(read = {
            reads.incrementAndGet()
            listOf(peer)
        }, remove = {})
        compose.setContent {
            MaterialTheme { SavedProfiles("owner", collapsed, calls, opened::add) }
        }
        compose.onNodeWithTag("saved-open-$peer").assertDoesNotExist()
        assertEquals(0, reads.get())
        compose.onNodeWithTag("saved-profiles-toggle").performClick()
        ready()
        compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
        assertEquals(1, reads.get())
        compose.onNodeWithTag("saved-open-$peer").performClick()
        compose.runOnIdle { assertEquals(listOf(peer), opened) }
        assertEquals(1, reads.get())
    }

    @Test
    fun pendingRemovalAndRemovedRowsRejectCapturedOpenCallbacks() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val peers = AtomicReference(listOf(peer))
        val opened = mutableListOf<String>()
        val calls = SavedProfilesCalls(read = { peers.get() }, remove = {
            assertEquals(peer, it)
            started.complete(Unit)
            release.await()
            peers.set(emptyList())
        })
        compose.setContent { MaterialTheme { SavedProfiles("owner", expanded, calls, opened::add) } }
        ready()
        val oldOpen = compose.onNodeWithTag("saved-open-$peer").fetchSemanticsNode().config[applicationClickKey]
        try {
            compose.onNodeWithTag("saved-remove-$peer").performClick()
            compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
            compose.onNodeWithTag("saved-open-$peer").assertIsNotEnabled()
            compose.runOnIdle { oldOpen() }
            compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
        } finally {
            release.complete(Unit)
        }
        ready()
        compose.onNodeWithTag("saved-open-$peer").assertDoesNotExist()
        compose.runOnIdle { oldOpen() }
        compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
    }

    @Test
    fun ownerReplacementCancelsAnOldLocalRead() {
        val started = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        val result = CompletableDeferred<List<String>>()
        var owner by mutableStateOf("old")
        val old = SavedProfilesCalls(read = {
            started.complete(Unit)
            try { result.await() } finally { returned.complete(Unit) }
        }, remove = {})
        val current = SavedProfilesCalls(read = { emptyList() }, remove = {})
        compose.setContent {
            MaterialTheme { SavedProfiles(owner, expanded, if (owner == "old") old else current, {}) }
        }
        compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
        compose.runOnIdle { owner = "new" }
        ready()
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        result.complete(listOf(peer))
        compose.waitForIdle()
        compose.onNodeWithTag("saved-open-$peer").assertDoesNotExist()
    }
}
