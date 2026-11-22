package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger

/** Real composition lifetime with controlled IO callbacks; core/JNI tests own disclosure policy. */
class ProfileViewDisclosureTests {
    @get:Rule
    val compose = createComposeRule()

    private class Attempt {
        val entered = CompletableDeferred<Unit>()
        val completed = CompletableDeferred<Unit>()
        val sends = AtomicInteger()
        val retirements = AtomicInteger()
        var release: CountDownLatch? = null
        val calls = ProfileViewDisclosureCalls(
            retire = { retirements.incrementAndGet() },
            send = {
                sends.incrementAndGet()
                entered.complete(Unit)
                release?.await()
                completed.complete(Unit)
            }
        )
    }

    @Test
    fun onlyTheFirstShownProfileStartsTheOpening() {
        val opening = Attempt()
        var shown by mutableStateOf(false)
        compose.setContent { ProfileViewDisclosureEffect(opening.calls, shown) }
        compose.runOnIdle { assertEquals(0, opening.sends.get()) }
        compose.runOnIdle { shown = true }
        awaitStep(opening.completed)
        compose.runOnIdle { shown = false }
        compose.runOnIdle { shown = true }
        compose.runOnIdle {
            assertEquals(1, opening.sends.get())
            assertEquals(0, opening.retirements.get())
        }
    }

    @Test
    fun disposingBeforeReadinessRetiresTheOldOpeningWithoutSubmittingIt() {
        val old = Attempt()
        val fresh = Attempt()
        var selected by mutableStateOf(old.calls)
        var shown by mutableStateOf(false)
        compose.setContent { ProfileViewDisclosureEffect(selected, shown) }
        compose.runOnIdle { selected = fresh.calls }
        compose.runOnIdle {
            assertEquals(1, old.retirements.get())
            assertEquals(0, old.sends.get())
            shown = true
        }
        awaitStep(fresh.completed)
        compose.runOnIdle {
            assertEquals(0, old.sends.get())
            assertEquals(1, fresh.sends.get())
        }
    }

    @Test
    fun anAdmittedOriginalCallbackFinishesWithoutRetargetingTheReplacement() {
        val release = CountDownLatch(1)
        val old = Attempt().apply { this.release = release }
        val fresh = Attempt()
        var selected by mutableStateOf(old.calls)
        try {
            compose.setContent { ProfileViewDisclosureEffect(selected, true) }
            awaitStep(old.entered)
            compose.runOnIdle { selected = fresh.calls }
            awaitStep(fresh.completed)
            compose.runOnIdle {
                assertEquals(1, old.retirements.get())
                assertEquals(1, fresh.sends.get())
            }
            release.countDown()
            awaitStep(old.completed)
            compose.runOnIdle {
                assertEquals(1, old.sends.get())
                assertEquals(1, fresh.sends.get())
            }
        } finally {
            release.countDown()
        }
    }

    @Test
    fun anUnavailableOpeningDoesNotScheduleAWorker() {
        var calls by mutableStateOf<ProfileViewDisclosureCalls?>(null)
        val opening = Attempt()
        compose.setContent { ProfileViewDisclosureEffect(calls, true) }
        compose.runOnIdle { calls = opening.calls }
        awaitStep(opening.completed)
        compose.runOnIdle { calls = null }
        compose.runOnIdle {
            assertEquals(1, opening.sends.get())
            assertEquals(1, opening.retirements.get())
        }
    }

    private fun awaitStep(signal: Deferred<Unit>) =
        compose.waitUntil(CHECK_WAIT_MS) { signal.isCompleted }
}
