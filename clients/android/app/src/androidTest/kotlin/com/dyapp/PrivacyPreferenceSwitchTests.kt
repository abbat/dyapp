package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import com.dyapp.ui.theme.DYAppTheme
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

class PrivacyPreferenceSwitchTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun unknownViewsRequireReadRetryAndCannotWriteAGuessedOffValue() {
        val kind = PrivacyPreferenceKind.VIEWS
        val fail = AtomicBoolean(true)
        val writes = AtomicInteger()
        val calls = PrivacyPreferenceCalls(
            read = {
                check(!fail.get()) { "controlled read failure" }
                false
            },
            write = { writes.incrementAndGet() }
        )
        compose.setContent { DYAppTheme { PrivacyPreferenceSwitch("owner", kind, calls) } }
        compose.awaitNode(hasTestTag("profile-views-retry") and isEnabled())
        compose.onNodeWithTag(kind.tag).assertIsNotEnabled()
        val blocked = action(kind)
        compose.runOnIdle { blocked() }
        assertEquals(0, writes.get())
        fail.set(false)
        compose.onNodeWithTag("profile-views-retry").performClick()
        ready(kind, false).assertIsOff()
        compose.runOnIdle { blocked() }
        assertEquals(0, writes.get())
    }

    @Test
    fun committedButUnconfirmedReceiptWriteNeedsReadbackBeforeAnotherToggle() {
        val kind = PrivacyPreferenceKind.RECEIPTS
        val stored = AtomicBoolean(false)
        val writes = AtomicInteger()
        val calls = PrivacyPreferenceCalls(read = { stored.get() }, write = {
            writes.incrementAndGet()
            stored.set(it)
            error("write committed before failure")
        })
        compose.setContent { DYAppTheme { PrivacyPreferenceSwitch("owner", kind, calls) } }
        val old = ready(kind, false).fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { old() }
        compose.awaitNode(hasTestTag("receipts-retry") and isEnabled())
        compose.onNodeWithTag(kind.tag).assertIsNotEnabled()
        val blocked = action(kind)
        compose.runOnIdle { blocked() }
        assertEquals(1, writes.get())
        compose.onNodeWithTag("receipts-retry").performClick()
        ready(kind, true).assertIsOn()
        compose.runOnIdle { old() }
        assertEquals(1, writes.get())
    }

    @Test
    fun pendingTypingRejectsDuplicatesAndRetiredOwnerCannotUpdateTheReplacement() {
        val kind = PrivacyPreferenceKind.TYPING
        val owner = mutableStateOf<Any>(Any())
        val first = owner.value
        val pending = PendingPrivacyWrite()
        val otherWrites = AtomicInteger()
        val other = PrivacyPreferenceCalls(read = { false }, write = { otherWrites.incrementAndGet() })
        compose.setContent {
            val calls = if (owner.value === first) pending.calls else other
            DYAppTheme { PrivacyPreferenceSwitch(owner.value, kind, calls) }
        }
        val old = ready(kind, false).fetchSemanticsNode().config[applicationClickKey]
        try {
            compose.runOnIdle { repeat(2) { old() } }
            compose.waitUntil(10_000) { pending.started.isCompleted }
            assertEquals(1, pending.writes.get())
            compose.runOnIdle { owner.value = Any() }
            ready(kind, false).assertIsOff()
            compose.runOnIdle { old() }
        } finally {
            pending.release.complete(Unit)
        }
        compose.waitUntil(10_000) { pending.returned.isCompleted }
        ready(kind, false).assertIsOff()
        assertEquals(1, pending.writes.get())
        assertEquals(0, otherWrites.get())
    }

    private fun ready(kind: PrivacyPreferenceKind, on: Boolean) = compose.awaitNode(
        hasTestTag(kind.tag) and isEnabled() and SemanticsMatcher.expectValue(
            SemanticsProperties.ToggleableState,
            if (on) ToggleableState.On else ToggleableState.Off
        )
    )

    private fun action(kind: PrivacyPreferenceKind) =
        compose.onNodeWithTag(kind.tag).fetchSemanticsNode().config[applicationClickKey]
}

private class PendingPrivacyWrite {
    val started = CompletableDeferred<Unit>()
    val release = CompletableDeferred<Unit>()
    val returned = CompletableDeferred<Unit>()
    val writes = AtomicInteger()
    private val stored = AtomicBoolean(false)
    val calls = PrivacyPreferenceCalls(read = { stored.get() }, write = {
        writes.incrementAndGet()
        currentCoroutineContext().job.invokeOnCompletion { returned.complete(Unit) }
        started.complete(Unit)
        withContext(NonCancellable) { release.await() }
        stored.set(it)
    })
}
