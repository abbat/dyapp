package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class NetworkCancellationTests {
    @get:Rule
    val compose = createComposeRule()
    private var visible by mutableStateOf(true)

    private fun show(fixture: NetworkCancellationFixture) {
        compose.setContent {
            if (visible) NetworkDraftScreen(fixture.opened) {} else Text("closed", Modifier.testTag("network-disposed"))
        }
        compose.awaitNode(hasTestTag("check-connection") and isEnabled())
        compose.onNodeWithTag("seeds").performTextReplacement("not an address")
    }

    private fun status(resource: Int) {
        val text = InstrumentationRegistry.getInstrumentation().targetContext.getString(resource)
        compose.awaitNode(hasTestTag("network-check-status") and hasText(text))
    }

    private fun start(fixture: NetworkCancellationFixture) {
        fixture.gate.arm()
        compose.onNodeWithTag("check-connection").performSemanticsAction(SemanticsActions.OnClick)
        fixture.gate.awaitEntered()
    }

    private fun closeScreen(fixture: NetworkCancellationFixture) {
        try {
            compose.runOnIdle { visible = false }
            compose.onNodeWithTag("network-disposed").assertExists()
        } finally {
            fixture.gate.release()
            fixture.awaitAdmissionRelease()
        }
    }

    @Test
    fun cancelDuringAdmissionWritesNothingAndTheNextRetryIsIndependent() {
        NetworkCancellationFixture().use { fixture ->
            try {
                show(fixture)
                start(fixture)
                compose.onNodeWithTag("cancel-check").performSemanticsAction(SemanticsActions.OnClick)
                fixture.gate.release()
                status(R.string.network_cancelled)
                fixture.awaitAdmissionRelease()
                assertFalse(File(fixture.opened.slot.dir, "network-draft").exists())
                compose.awaitNode(
                    hasTestTag("check-connection") and isEnabled()
                ).performSemanticsAction(SemanticsActions.OnClick)
                status(R.string.network_invalid_address)
                fixture.awaitAdmissionRelease()
                assertTrue(File(fixture.opened.slot.dir, "network-draft").isFile)
            } finally {
                closeScreen(fixture)
            }
        }
    }

    @Test
    fun disposalCancelsTheAdmittedAttemptBeforeItsBlockedKeyOpenCompletes() {
        NetworkCancellationFixture().use { fixture ->
            try {
                show(fixture)
                start(fixture)
            } finally {
                closeScreen(fixture)
            }
            assertFalse(File(fixture.opened.slot.dir, "network-draft").exists())
            assertFalse(File(fixture.opened.slot.dir, "network").exists())
        }
    }
}
