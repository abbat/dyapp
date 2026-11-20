package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ChatMediaCacheResult
import java.util.concurrent.atomic.AtomicInteger

class MediaCacheOwnerTests {
    @get:Rule
    val compose = createComposeRule()

    private fun release(release: CompletableDeferred<Unit>, returned: CompletableDeferred<Unit>) {
        release.complete(Unit)
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        compose.waitForIdle()
    }

    @Test
    fun cancellationOfConfirmationDoesNotSubmitAConfirmedEdit() {
        val requests = AtomicInteger()
        val confirmed = AtomicInteger()
        val calls = MediaCacheCalls(read = { cacheSettings() }, change = { _, allow ->
            requests.incrementAndGet()
            if (allow) confirmed.incrementAndGet()
            ChatMediaCacheResult(cacheSettings(), true)
        })
        compose.setContent { MediaCacheTestHost("owner", calls) }
        compose.cacheClick("cache-enabled")
        compose.cacheReady("cache-confirm")
        assertEquals(1, requests.get())
        assertEquals(0, confirmed.get())
        compose.cacheReady("cache-cancel").performClick()
        compose.cacheReady("cache-enabled")
        compose.onNodeWithTag("cache-confirm").assertDoesNotExist()
        assertEquals(1, requests.get())
        assertEquals(0, confirmed.get())
    }

    @Test
    fun oldConfirmationCannotAuthorizeANewerDialogOpening() {
        val confirmed = AtomicInteger()
        val initial = cacheSettings()
        val off = initial.copy(state = initial.state.copy(enabled = false, usedBytes = 0uL))
        val calls = MediaCacheCalls(read = { initial }, change = { _, allow ->
            if (allow) confirmed.incrementAndGet()
            ChatMediaCacheResult(if (allow) off else initial, !allow)
        })
        compose.setContent { MediaCacheTestHost("owner", calls) }
        compose.cacheClick("cache-enabled")
        compose.cacheReady("cache-confirm")
        val stale = compose.onNodeWithTag("cache-confirm").fetchSemanticsNode().config[SemanticsActions.OnClick]
        compose.cacheReady("cache-cancel").performClick()
        compose.cacheClick("cache-enabled")
        compose.cacheReady("cache-confirm")
        compose.runOnIdle { stale.action?.invoke() }
        compose.onNodeWithTag("cache-confirm").assertIsEnabled()
        assertEquals(0, confirmed.get())
        compose.cacheReady("cache-confirm").performClick()
        compose.cacheReady("cache-enabled")
        compose.onNodeWithTag("cache-confirm").assertDoesNotExist()
        assertEquals(1, confirmed.get())
    }

    @Test
    fun repeatedConfirmationAndLateNativeCompletionStayWithTheOriginalIdentity() {
        val started = CompletableDeferred<Unit>()
        val finish = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        val writes = AtomicInteger()
        try {
            var owner by mutableStateOf("first")
            val first = gatedCalls(started, finish, returned, writes)
            val second = MediaCacheCalls(read = { cacheSettings("512", 536_870_912uL) }, change = { _, _ ->
                error("unexpected successor mutation")
            })
            compose.setContent { MediaCacheTestHost(owner, if (owner == "first") first else second) }
            compose.cacheClick("cache-enabled")
            compose.cacheReady("cache-confirm")
            val action = compose.onNodeWithTag("cache-confirm").fetchSemanticsNode().config[SemanticsActions.OnClick]
            compose.runOnIdle { repeat(2) { action.action?.invoke() } }
            compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
            compose.onNodeWithTag("cache-cancel").assertIsNotEnabled()
            assertEquals(1, writes.get())
            compose.runOnIdle { owner = "second" }
            compose.cacheReady("cache-limit")
            assertEquals("512", compose.cacheDraft())
            release(finish, returned)
            assertEquals("512", compose.cacheDraft())
        } finally { finish.complete(Unit) }
    }

    private fun gatedCalls(
        started: CompletableDeferred<Unit>,
        finish: CompletableDeferred<Unit>,
        returned: CompletableDeferred<Unit>,
        writes: AtomicInteger
    ) = MediaCacheCalls(read = { cacheSettings() }, change = { _, confirmed ->
        if (!confirmed) {
            ChatMediaCacheResult(cacheSettings(), true)
        } else {
            writes.incrementAndGet()
            started.complete(Unit)
            try {
                withContext(NonCancellable) { finish.await() }
                ChatMediaCacheResult(cacheSettings(), false)
            } finally { returned.complete(Unit) }
        }
    })
}
