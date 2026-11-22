package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CountDownLatch

class LinkApprovalUILifetimeTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun admittedSendIgnoresBackAndOldCallbacksCannotActOnAnEqualSuccessor() {
        val fixture = LinkApprovalUIFixture(compose)
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        fixture.original.sending.set {
            entered.complete(Unit)
            release.await()
        }
        try {
            val callbacks = openAndCapture(fixture)
            compose.onNodeWithTag("owner-send").performClick()
            runBlocking { withTimeout(5_000) { entered.await() } }
            assertBusyDialog()
            fixture.replaceOwner()
            assertTrue(fixture.original.session.state.value.retired)
            assertEquals(0, fixture.original.closed.get())
            checkSuccessor(fixture, callbacks, release)
        } finally {
            release.countDown()
            runBlocking {
                fixture.original.close()
                fixture.successor.close()
            }
        }
    }

    private fun openAndCapture(fixture: LinkApprovalUIFixture): List<() -> Unit> {
        fixture.show()
        compose.awaitNode(hasTestTag("owner-open"))
        val open = fixture.callback("owner-open")
        fixture.enterCode()
        return listOf(open, fixture.callback("owner-send"), fixture.callback("owner-cancel"))
    }

    private fun assertBusyDialog() {
        compose.awaitNode(hasTestTag("owner-pending"))
        compose.onNodeWithTag("owner-cancel").assertIsNotEnabled()
        compose.onNodeWithTag("owner-send").assertIsNotEnabled()
        Espresso.pressBack()
        compose.waitForIdle()
        compose.awaitNode(hasTestTag("owner-dialog"))
    }

    private fun checkSuccessor(
        fixture: LinkApprovalUIFixture,
        callbacks: List<() -> Unit>,
        release: CountDownLatch
    ) {
        compose.onNodeWithTag("owner-open").performClick()
        compose.awaitNode(hasTestTag("owner-code-input"))
        release.countDown()
        runBlocking { fixture.original.close() }
        compose.runOnIdle { callbacks.forEach { it.invoke() } }
        compose.awaitNode(hasTestTag("owner-code-input"))
        compose.onNodeWithTag("owner-sent").assertDoesNotExist()
        assertEquals(1, fixture.original.sent.get())
        assertEquals(1, fixture.original.closed.get())
        assertEquals(0, fixture.successor.created.get())
        assertEquals(0, fixture.successor.sent.get())
        assertEquals(2, fixture.created.get())
    }
}
