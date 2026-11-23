package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.SearchRecoveryException
import uniffi.dyapp.SearchRecoverySaved
import java.util.concurrent.CountDownLatch

class SearchRecoveryUITests {
    @get:Rule
    val compose = createComposeRule()
    private val queued = SearchRecoverySaved(2uL, true, true)

    @Test
    fun explicitConfirmationAndClosingPreserveTheSameUnsavedEditor() {
        val original = SearchRecoveryUIOwner { queued }
        val successor = SearchRecoveryUIOwner { error("Unexpected successor confirmation") }
        val fixture = SearchRecoveryUIFixture(compose, original, successor)
        try {
            fixture.show()
            val editor = original.editor
            compose.runOnIdle { editor.name.value = "Unpublished name" }
            confirm()
            compose.awaitSnapshot(original.session.state, 5_000, "queued withdrawal") { it.saved != null && !it.busy }
            compose.waitUntil(5_000) { original.visibility.hidden }
            compose.onNodeWithTag("search-recovery-close").performScrollTo().performClick()
            compose.awaitNode(hasTestTag("profile-name")).assertTextContains("Unpublished name")
            assertSame(editor, original.editor)
            assertTrue(editor.unsaved)
            assertEquals(1, original.confirmed.get())
        } finally {
            original.close()
            successor.close()
        }
    }

    @Test
    fun lateOriginalCompletionAndCapturedCallbacksCannotMutateAnEqualSuccessor() {
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val original = SearchRecoveryUIOwner {
            entered.complete(Unit)
            release.await()
            queued
        }
        val successor = SearchRecoveryUIOwner { error("Unexpected successor confirmation") }
        val fixture = SearchRecoveryUIFixture(compose, original, successor)
        try {
            fixture.show()
            val callbacks = callbacks(fixture)
            confirm()
            compose.waitUntil(5_000) { entered.isCompleted }
            compose.onNodeWithTag("search-recovery-close").assertIsNotEnabled()
            fixture.replaceOwner()
            assertOriginalRetired(original)
            release.countDown()
            original.close()
            compose.runOnIdle { callbacks.forEach { it() } }
            assertSuccessorUnchanged(successor)
            assertOriginalCompleted(original)
        } finally {
            release.countDown()
            original.close()
            successor.close()
        }
    }

    @Test
    fun uncertainConfirmationOffersCloseWithoutClaimingToCancelSavedWork() {
        val original = SearchRecoveryUIOwner { throw SearchRecoveryException.Failed("Controlled uncertain readback") }
        val successor = SearchRecoveryUIOwner { error("Unexpected successor confirmation") }
        val fixture = SearchRecoveryUIFixture(compose, original, successor)
        try {
            fixture.show()
            confirm()
            compose.awaitSnapshot(original.session.state, 5_000, "uncertain confirmation") { it.failure != null }
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            compose.onNodeWithTag(
                "search-recovery-close"
            ).assertTextContains(context.getString(R.string.search_recovery_close))
            assertEquals(1, original.confirmed.get())
        } finally {
            original.close()
            successor.close()
        }
    }

    private fun assertOriginalCompleted(owner: SearchRecoveryUIOwner) {
        assertEquals(1, owner.confirmed.get())
        assertEquals(1, owner.closed.get())
    }

    private fun assertOriginalRetired(owner: SearchRecoveryUIOwner) {
        assertTrue(owner.session.state.value.retired)
        assertEquals(0, owner.closed.get())
    }

    private fun callbacks(fixture: SearchRecoveryUIFixture) = listOf(
        fixture.callback("search-recovery-action"),
        fixture.callback("search-recovery-close")
    )

    private fun confirm() = compose.onNodeWithTag("search-recovery-action").performScrollTo().performClick()

    private fun assertSuccessorUnchanged(owner: SearchRecoveryUIOwner) {
        compose.onNodeWithTag("search-recovery-action").assertIsEnabled()
        compose.onNodeWithTag("search-recovery-close").assertIsEnabled()
        compose.onNodeWithTag("profile-name").assertTextContains("Saved")
        assertEquals(0, owner.confirmed.get())
        assertEquals(0, owner.closed.get())
        assertEquals(1uL, owner.visibility.version)
        assertEquals(false, owner.visibility.hidden)
        assertEquals(null, owner.session.state.value.saved)
    }
}
