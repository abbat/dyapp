package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertDoesNotExist
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CountDownLatch

class DeviceDirectoryUITests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun retiredDialogCallbackCannotAffectAnEqualSuccessorAfterTheNativeWriteCompletes() {
        val original = DeviceDirectoryFixture()
        val successor = DeviceDirectoryFixture()
        val shown = mutableStateOf(original.session)
        val entered = CompletableDeferred<ByteArray>()
        val release = CountDownLatch(1)
        holdWrite(original, entered, release)
        try {
            show(shown)
            openOther()
            val confirm = compose.awaitNode(hasTestTag("disconnect-confirm"))
            val callback = confirm.fetchSemanticsNode().config[applicationClickKey]
            confirm.performClick()
            assertArrayEquals(original.other, runBlocking { withTimeout(5_000) { entered.await() } })
            compose.awaitNode(hasTestTag("disconnect-pending"))
            compose.onNodeWithTag("disconnect-cancel").assertIsNotEnabled()
            compose.onNodeWithTag("disconnect-confirm").assertIsNotEnabled()
            replaceAndComplete(shown, original, successor, release)
            checkSuccessor(callback, original, successor)
        } finally {
            release.countDown()
            runBlocking {
                original.close()
                successor.close()
            }
        }
    }

    @Test
    fun partialFailureOffersExplicitRetryAndAcknowledgementSurvivesCacheReadFailure() {
        val fixture = DeviceDirectoryFixture()
        fixture.writes.set { error("mailbox closed before profile publication failed") }
        try {
            show(mutableStateOf(fixture.session))
            compose.awaitNode(hasTestTag("device-key-1"))
            compose.onNodeWithTag("device-disconnect-1").assertDoesNotExist()
            openOther()
            val target = checkNotNull(fixture.session.state.value.confirmation).target.identifier
            compose.onNodeWithTag("disconnect-device").assertTextEquals(target)
            compose.onNodeWithTag("disconnect-confirm").performClick()
            compose.awaitNode(hasTestTag("disconnect-problem")).assertTextEquals(text(R.string.devices_uncertain))
            compose.onNodeWithTag("disconnect-retry").assertIsEnabled()
            assertEquals(1, fixture.writeCount.get())
            fixture.writes.set {}
            fixture.reads.set { error("cache unavailable after write acknowledgement") }
            compose.onNodeWithTag("disconnect-retry").performClick()
            assertAcknowledged(target)
            assertEquals(2, fixture.writeCount.get())
        } finally {
            runBlocking { fixture.close() }
        }
    }

    private fun assertAcknowledged(target: String) {
        compose.awaitNode(hasTestTag("devices-completion") and hasText(text(R.string.devices_gone)))
            .assertTextEquals(text(R.string.devices_gone))
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite))
        compose.awaitNode(hasTestTag("devices-read-error"))
        compose.onNodeWithTag("devices-completion-target").assertTextEquals(target)
        compose.onNodeWithTag("disconnect-dialog").assertDoesNotExist()
    }

    private fun holdWrite(
        original: DeviceDirectoryFixture,
        entered: CompletableDeferred<ByteArray>,
        release: CountDownLatch
    ) {
        original.writes.set { key ->
            entered.complete(key.copyOf())
            release.await()
        }
    }

    private fun checkSuccessor(
        callback: () -> Unit,
        original: DeviceDirectoryFixture,
        successor: DeviceDirectoryFixture
    ) {
        compose.runOnIdle { callback.invoke() }
        compose.awaitNode(hasTestTag("device-key-1"))
        compose.onNodeWithTag("disconnect-dialog").assertDoesNotExist()
        compose.onNodeWithTag("devices-completion").assertDoesNotExist()
        assertEquals(1, original.writeCount.get())
        assertEquals(0, successor.writeCount.get())
    }

    private fun show(owner: MutableState<DeviceDirectorySession>) {
        compose.setContent {
            MaterialTheme {
                Column(Modifier.verticalScroll(rememberScrollState())) { DeviceDirectoryScreen(owner.value) }
            }
        }
    }

    private fun openOther() {
        compose.awaitNode(hasTestTag("device-disconnect-2")).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("disconnect-dialog"))
    }

    private fun replaceAndComplete(
        shown: MutableState<DeviceDirectorySession>,
        original: DeviceDirectoryFixture,
        successor: DeviceDirectoryFixture,
        release: CountDownLatch
    ) {
        compose.runOnIdle { shown.value = successor.session }
        compose.waitForIdle()
        assertTrue(original.session.state.value.retired)
        assertEquals(0, original.closeCount.get())
        release.countDown()
        runBlocking { original.close() }
        assertEquals(1, original.closeCount.get())
    }

    private fun text(id: Int): String = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)
}
