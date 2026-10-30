package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

class IdentitySettingsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun active() = slots.open() as Opened.Ready

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    private fun switchTo(peerId: String) = compose.onNode(
        hasTestTag("settings-switch") and
            hasAnyAncestor(hasTestTag("settings-identity") and hasAnyDescendant(hasText(peerId)))
    ).performScrollTo().performClick()

    @Test
    fun createSwitchAndConfirmedRemoveChangeTheActiveIdentity() {
        val first = compose.shownPeerId()
        click("settings-open")
        val rows = compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size
        click("settings-create")
        compose.waitForIdle()
        val second = active()
        check(second.peerId != first) { "Create kept the previous identity active" }
        check(compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size == rows + 1)
        switchTo(first)
        check(active().peerId == first) { "Switch did not activate the chosen identity" }
        switchTo(second.peerId)
        check(active().peerId == second.peerId) { "Switch did not return to the new identity" }
        // Cancel returns to Settings with the identity intact.
        click("settings-remove")
        compose.onNodeWithTag("remove-action").assertIsNotEnabled()
        click("remove-cancel")
        compose.onNodeWithTag("settings-back").assertExists()
        check(active().peerId == second.peerId) { "Cancel changed the active identity" }
        click("settings-remove")
        click("remove-confirm")
        click("remove-action")
        compose.onNodeWithTag("app-ready").assertTextEquals(compose.activity.getString(R.string.status_pick))
        check(!PlatformKeyStore.exists(second.slot.alias)) { "The removed identity kept its key" }
    }
}
