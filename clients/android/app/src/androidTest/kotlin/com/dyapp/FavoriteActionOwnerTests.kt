package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

class FavoriteActionOwnerTests {
    @get:Rule
    val compose = createComposeRule()

    private fun ready() = compose.awaitNode(hasTestTag("view-favorite-toggle") and isEnabled())

    private fun text(resource: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(resource)

    @Test
    fun pendingWriteAdmitsOnceAndReadbackControlsTheSavedLabel() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val saved = AtomicBoolean(false)
        val writes = AtomicInteger()
        val calls = FavoriteCalls(
            read = { saved.get() },
            write = {
                writes.incrementAndGet()
                started.complete(Unit)
                release.await()
                saved.set(it)
            }
        )
        compose.setContent { MaterialTheme { FavoriteAction("first-opening", calls) } }
        ready()
        val action = compose.onNodeWithTag("view-favorite-toggle").fetchSemanticsNode().config[SemanticsActions.OnClick]
        compose.runOnIdle { repeat(2) { action.action?.invoke() } }
        compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
        compose.onNodeWithTag("view-favorite-toggle").assertIsNotEnabled()
        assertEquals(1, writes.get())
        release.complete(Unit)
        ready()
        compose.onNodeWithTag("view-favorite-toggle").assertTextEquals(text(R.string.favorites_remove))
        compose.onNodeWithTag("view-favorite-status").assertTextEquals(text(R.string.favorites_saved))
        assertEquals(true, saved.get())
    }

    @Test
    fun oldCompletionCannotReplaceTheNewOwnersUnsavedState() {
        val started = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        val oldResult = CompletableDeferred<Boolean>()
        var owner by mutableStateOf("old")
        val old = FavoriteCalls(read = {
            started.complete(Unit)
            try { oldResult.await() } finally { returned.complete(Unit) }
        }, write = {})
        val current = FavoriteCalls(read = { false }, write = {})
        compose.setContent { MaterialTheme { FavoriteAction(owner, if (owner == "old") old else current) } }
        compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
        compose.runOnIdle { owner = "new" }
        ready()
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        compose.onNodeWithTag("view-favorite-toggle").assertTextEquals(text(R.string.favorites_add))
        oldResult.complete(true)
        compose.waitForIdle()
        compose.onNodeWithTag("view-favorite-toggle").assertTextEquals(text(R.string.favorites_add))
    }

    @Test
    fun aFailedReadOffersOnlyReadRetryBeforeAnyMutation() {
        val reads = AtomicInteger()
        val writes = AtomicInteger()
        val calls = FavoriteCalls(
            read = {
                check(reads.incrementAndGet() > 1) { "controlled read failure" }
                true
            },
            write = { writes.incrementAndGet() }
        )
        compose.setContent { MaterialTheme { FavoriteAction("retry-opening", calls) } }
        ready()
        compose.onNodeWithTag("view-favorite-toggle").assertTextEquals(text(R.string.favorites_retry))
        compose.onNodeWithTag("view-favorite-status").assertTextEquals(text(R.string.favorites_load_failed))
        compose.onNodeWithTag("view-favorite-toggle").performClick()
        ready()
        compose.onNodeWithTag("view-favorite-toggle").assertTextEquals(text(R.string.favorites_remove))
        assertEquals(2, reads.get())
        assertEquals(0, writes.get())
    }
}
