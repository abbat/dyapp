package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class LinkReceiveScreenTests {
    @get:Rule
    val compose = createComposeRule()

    private fun action(tag: String): () -> Unit =
        compose.awaitNode(hasTestTag(tag)).fetchSemanticsNode().config[applicationClickKey]

    @Test
    fun destinationFailureRetainsTheOriginalOutcomeAndRetiredStartCannotRepeat() {
        val fixture = LinkReceiveFixture(compose)
        try {
            fixture.show()
            val start = action("link-receive-start")
            assertNull(fixture.store.state.value)
            val opening = fixture.start()
            compose.onNodeWithTag("link-receive-destination").performScrollTo().assertIsDisplayed()
                .assertTextEquals(fixture.destination)
            compose.onNodeWithTag("link-receive-open").assertDoesNotExist()
            compose.runOnIdle {
                start()
                assertSame(opening, fixture.store.state.value)
                assertEquals(1, fixture.creates.get())
                assertEquals(1, fixture.polls.get())
                assertEquals(0, fixture.returned)
            }
        } finally {
            fixture.close()
        }
    }

    @Test
    fun disposedScreenCannotDismissOrNavigateItsRetainedOutcome() {
        val fixture = LinkReceiveFixture(compose)
        try {
            fixture.show()
            val opening = fixture.start()
            val dismiss = action("link-receive-dismiss")
            val review = action("link-receive-review")
            compose.runOnIdle { fixture.mounted = false }
            compose.onNodeWithTag("settings-receive").assertDoesNotExist()
            compose.runOnIdle {
                dismiss()
                review()
                assertSame(opening, fixture.store.state.value)
                assertEquals(0, fixture.reviews)
                assertEquals(0, fixture.returned)
            }
        } finally {
            fixture.close()
        }
    }

    @Test
    fun currentExplicitDispositionClosesOnceAndReturnsToAccount() {
        val fixture = LinkReceiveFixture(compose)
        try {
            fixture.show()
            fixture.start()
            val dismiss = action("link-receive-dismiss")
            compose.runOnIdle { dismiss() }
            compose.awaitNode(hasTestTag("settings-account"))
            compose.runOnIdle {
                dismiss()
                assertNull(fixture.store.state.value)
                assertTrue(fixture.route.value == SettingsPane.ACCOUNT)
                assertEquals(1, fixture.closes.get())
            }
        } finally {
            fixture.close()
        }
    }

    @Test
    fun anOldDispositionCannotDismissAnEqualReopenedSource() {
        val fixture = LinkReceiveFixture(compose)
        try {
            fixture.show()
            fixture.start()
            val dismiss = action("link-receive-dismiss")
            val review = action("link-receive-review")
            runBlocking { fixture.store.release(checkNotNull(fixture.store.state.value)) }
            compose.awaitNode(hasTestTag("link-receive-start"))
            val replacement = fixture.start()
            compose.runOnIdle {
                dismiss()
                review()
                assertSame(replacement, fixture.store.state.value)
                assertEquals(0, fixture.reviews)
                assertEquals(2, fixture.creates.get())
            }
        } finally {
            fixture.close()
        }
    }
}
