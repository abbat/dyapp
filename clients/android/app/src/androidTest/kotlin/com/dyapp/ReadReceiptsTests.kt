package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
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

    @Test
    fun theReadReceiptSwitchIsOffByDefaultAndStored() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val switch = compose.onNodeWithTag("send-read-receipts").performScrollTo()
        fun waitFor(on: Boolean) = compose.waitUntil(5_000) {
            runCatching { (if (on) switch.assertIsOn() else switch.assertIsOff()).assertIsEnabled() }.isSuccess
        }
        waitFor(false)
        switch.performClick()
        waitFor(true)
        check(stored()) { "On was not stored" }
        compose.activityRule.scenario.recreate()
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-settings").performClick()
        waitFor(true)
        switch.performScrollTo().performClick()
        waitFor(false)
        check(!stored()) { "Off was not stored" }
        compose.onNodeWithTag("receipts-error").assertDoesNotExist()
    }
}
