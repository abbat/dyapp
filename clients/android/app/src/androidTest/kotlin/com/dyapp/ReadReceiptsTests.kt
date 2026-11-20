package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class ReadReceiptsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun stored() = RustBridge.withChats(slots.open() as Opened.Ready) { it.readReceipts() }

    private fun ready(on: Boolean) = compose.awaitNode(
        hasTestTag("send-read-receipts") and isEnabled() and
            SemanticsMatcher.expectValue(
                SemanticsProperties.ToggleableState,
                if (on) ToggleableState.On else ToggleableState.Off
            )
    )

    private fun changeTo(on: Boolean) {
        ready(!on).performScrollTo().performSemanticsAction(SemanticsActions.OnClick)
        ready(on)
    }

    @Test
    fun theReadReceiptSwitchIsOffByDefaultAndStored() {
        compose.shownPeerId()
        compose.openSettingsPane("settings-privacy")
        changeTo(true)
        check(stored()) { "On was not stored" }
        compose.activityRule.scenario.recreate()
        compose.skipNetworkEntry()
        compose.openSettingsPane("settings-privacy")
        changeTo(false)
        check(!stored()) { "Off was not stored" }
        compose.onNodeWithTag("receipts-error").assertDoesNotExist()
    }
}
