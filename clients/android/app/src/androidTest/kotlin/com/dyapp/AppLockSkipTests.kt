package com.dyapp

import android.content.Context
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

class AppLockSkipTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishSkipWaitsForOneCompletionAndShowsTheFailure() = checkLanguage("en")

    @Test
    fun russianSkipWaitsForOneCompletionAndShowsTheFailure() = checkLanguage("ru")

    private fun checkLanguage(language: String) {
        val finished = CompletableDeferred<Boolean>()
        val entered = MutableStateFlow(false)
        val attempts = AtomicInteger()
        val context = show(language) {
            attempts.incrementAndGet()
            entered.value = true
            finished.await()
        }
        try {
            val click = compose.onNodeWithTag("lock-skip").fetchSemanticsNode().config[applicationClickKey]
            compose.runOnIdle {
                click()
                click()
            }
            compose.awaitSnapshot(entered, 5_000, "Skip admission") { it }
            assertEquals(1, attempts.get())
            assertPending()
            finished.complete(false)
            compose.awaitNode(hasTestTag("lock-error")).assertTextEquals(context.getString(R.string.lock_skip_failed))
            assertEquals(1, attempts.get())
        } finally {
            finished.complete(false)
        }
    }

    private fun assertPending() {
        compose.onNodeWithTag("lock-skip").assertIsNotEnabled()
        compose.onNodeWithTag("lock-set").assertIsNotEnabled()
        compose.onNodeWithTag("pin-set").assertIsNotEnabled()
    }

    private fun show(language: String, skip: suspend () -> Boolean): Context {
        val context = ownerVisualContext(language, 1f)
        compose.setContent {
            LocalizedTestContext(context, 1f) {
                MaterialTheme { SetupScreen(DeviceLock(true), skip) { true } }
            }
        }
        return context
    }
}
