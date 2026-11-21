package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
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
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

class PresenceCardTests {
    @get:Rule
    val compose = createComposeRule()

    private fun ready(tag: String) = compose.awaitNode(hasTestTag(tag) and isEnabled())

    private fun selected(tag: String) {
        ready(tag)
        compose.onNodeWithTag(tag).assertIsSelected()
    }

    @Test
    fun unknownAudienceRequiresReadRetryInsteadOfWritingADefault() {
        val fail = AtomicBoolean(true)
        val writes = AtomicInteger()
        val calls = PrivacyPreferenceCalls(read = {
            check(!fail.get()) { "controlled read failure" }
            false
        }, write = { writes.incrementAndGet() })
        compose.setContent { MaterialTheme { PresencePicker("owner", calls) } }
        ready("presence-retry")
        compose.onNodeWithTag("presence-matches").assertIsNotEnabled()
        compose.onNodeWithTag("presence-nobody").assertIsNotEnabled()
        assertEquals(0, writes.get())
        fail.set(false)
        compose.onNodeWithTag("presence-retry").performClick()
        selected("presence-nobody")
        assertEquals(0, writes.get())
    }

    @Test
    fun indeterminateSaveRequiresReadbackBeforeAnotherDisclosureChoice() {
        val stored = AtomicBoolean(true)
        val writes = AtomicInteger()
        val calls = PrivacyPreferenceCalls(read = { stored.get() }, write = {
            stored.set(it)
            writes.incrementAndGet()
            error("write committed before failure")
        })
        compose.setContent { MaterialTheme { PresencePicker("owner", calls) } }
        selected("presence-matches")
        compose.onNodeWithTag("presence-nobody").performClick()
        ready("presence-retry")
        compose.onNodeWithTag("presence-matches").assertIsNotEnabled()
        compose.onNodeWithTag("presence-nobody").assertIsNotEnabled()
        compose.onNodeWithTag("presence-retry").performClick()
        selected("presence-nobody")
        assertEquals(1, writes.get())
    }

    @Test
    fun doubleAdmissionAndOldCompletionCannotChangeTheNewOwner() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        val writes = AtomicInteger()
        val owner = mutableStateOf("first")
        val first = pendingCall(writes, started, release, returned)
        val second = PrivacyPreferenceCalls(read = { true }, write = {})
        showOwners(owner, first, second)
        try {
            ready("presence-nobody")
            val action = compose.onNodeWithTag("presence-nobody").fetchSemanticsNode().config[SemanticsActions.OnClick]
            compose.runOnIdle { repeat(2) { action.action?.invoke() } }
            compose.waitUntil(timeoutMillis = 10_000) { started.isCompleted }
            assertEquals(1, writes.get())
            compose.runOnIdle { owner.value = "second" }
            selected("presence-matches")
        } finally {
            release.complete(Unit)
        }
        compose.waitUntil(timeoutMillis = 10_000) { returned.isCompleted }
        selected("presence-matches")
        assertEquals(1, writes.get())
    }

    private fun pendingCall(
        writes: AtomicInteger,
        started: CompletableDeferred<Unit>,
        release: CompletableDeferred<Unit>,
        returned: CompletableDeferred<Unit>
    ) = PrivacyPreferenceCalls(read = { true }, write = {
        writes.incrementAndGet()
        currentCoroutineContext().job.invokeOnCompletion { returned.complete(Unit) }
        started.complete(Unit)
        withContext(NonCancellable) { release.await() }
    })

    private fun showOwners(
        owner: androidx.compose.runtime.MutableState<String>,
        first: PrivacyPreferenceCalls,
        second: PrivacyPreferenceCalls
    ) = compose.setContent {
        MaterialTheme { PresencePicker(owner.value, if (owner.value == "first") first else second) }
    }
}
