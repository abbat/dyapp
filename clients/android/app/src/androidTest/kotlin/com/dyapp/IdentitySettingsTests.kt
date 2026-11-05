package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test
import uniffi.dyapp.ChatNotice

class IdentitySettingsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun active() = slots.open() as Opened.Ready

    // The settings lists load after the screen opens, so a row may not exist yet.
    private fun click(tag: String) {
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag(tag).performScrollTo().performClick()
    }

    @Test
    fun createSwitchAndConfirmedRemoveChangeTheActiveIdentity() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val rows = compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size
        click("settings-create")
        compose.waitForIdle()
        val second = active()
        check(second.peerId != first) { "Create kept the previous identity active" }
        check(compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size == rows + 1)
        compose.switchIdentity(first)
        check(active().peerId == first) { "Switch did not activate the chosen identity" }
        // A relaunch keeps the identity chosen in Settings.
        compose.activityRule.scenario.recreate()
        check(compose.shownPeerId() == first) { "The identity chosen in Settings was lost on relaunch" }
        compose.onNodeWithTag("nav-settings").performClick()
        compose.switchIdentity(second.peerId)
        check(active().peerId == second.peerId) { "Switch did not return to the new identity" }
        // Cancel returns to Settings with the identity intact.
        click("settings-remove")
        compose.onNodeWithTag("remove-action").assertIsNotEnabled()
        compose.onNodeWithTag("remove-cancel").performClick()
        compose.onNodeWithTag("settings-create").assertExists()
        check(active().peerId == second.peerId) { "Cancel changed the active identity" }
        click("settings-remove")
        click("remove-confirm")
        compose.onNodeWithTag("remove-action").performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(compose.activity.getString(R.string.status_pick))
        check(!PlatformKeyStore.exists(second.slot.alias)) { "The removed identity kept its key" }
        // Later tests start from the active identity, not the picker.
        compose.onNodeWithText(shortPeerId(first)).performScrollTo().performClick()
        check(compose.shownPeerId() == first) { "The remaining identity did not open" }
    }

    @Test
    fun removingAnotherIdentityStaysInSettings() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val rows = compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size
        click("settings-create")
        compose.waitForIdle()
        val other = active()
        compose.switchIdentity(first)
        compose.onNode(
            hasText(compose.activity.getString(R.string.identity_remove)) and hasAnyAncestor(
                hasTestTag("settings-identity") and hasAnyDescendant(hasText(shortPeerId(other.peerId)))
            )
        ).performScrollTo().performClick()
        click("remove-confirm")
        compose.onNodeWithTag("remove-action").performClick()
        compose.onNodeWithTag("settings-create").assertExists()
        check(active().peerId == first) { "Removing another identity changed the active one" }
        check(!PlatformKeyStore.exists(other.slot.alias)) { "The removed identity kept its key" }
        check(compose.onAllNodesWithTag("settings-identity").fetchSemanticsNodes().size == rows)
    }

    @Test
    fun theNoticeSwitchIsStoredPerIdentity() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val switch = compose.onNodeWithTag("message-notifications").performScrollTo()
        // Until the store is read the switch shows off and disabled, so off alone is no proof of a load.
        fun waitFor(on: Boolean) = compose.waitUntil(5_000) {
            runCatching { (if (on) switch.assertIsOn() else switch.assertIsOff()).assertIsEnabled() }.isSuccess
        }
        // On by default; the emulator allows notices, so nothing asks for them.
        waitFor(true)
        compose.onNodeWithTag("notice-blocked").assertDoesNotExist()
        compose.onNodeWithTag("notice-fixed").assertIsSelected()
        click("notice-text")
        compose.waitUntil(5_000) { stored() == ChatNotice.TEXT }
        switch.performScrollTo().performClick()
        waitFor(false)
        check(stored() == ChatNotice.OFF) { "Off was not stored" }
        compose.activityRule.scenario.recreate()
        compose.onNodeWithTag("nav-settings").performClick()
        waitFor(false)
        // Off keeps the choice, disabled, and a turn-on brings it back.
        compose.onNodeWithTag("notice-text").performScrollTo().assertIsSelected().assertIsNotEnabled()
        switch.performScrollTo().performClick()
        waitFor(true)
        check(stored() == ChatNotice.TEXT) { "On lost the content choice" }
        click("notice-fixed")
        compose.waitUntil(5_000) { stored() == ChatNotice.FIXED }
    }

    private fun stored() = RustBridge.withChats(active()) { it.notice() }

    @Test
    fun undoPassEmptiesThePassedPeopleList() {
        compose.shownPeerId()
        val peer = "ab".repeat(32)
        RustBridge.withChats(active()) { it.setPassed(peer, true) }
        compose.onNodeWithTag("nav-settings").performClick()
        compose.onNodeWithTag("passed-none").assertDoesNotExist()
        click("unpass-1")
        compose.waitUntil(5_000) { compose.onAllNodesWithTag("passed-none").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("unpass-1").assertDoesNotExist()
        check(RustBridge.withChats(active()) { it.passedPeers() }.isEmpty()) { "Undo pass was not stored" }
    }

    @Test
    fun retryAndUnblockEmptyTheBlockedPeopleList() {
        compose.shownPeerId()
        val peer = "cd".repeat(32)
        RustBridge.withChats(active()) { it.block(peer) }
        compose.onNodeWithTag("nav-settings").performClick()
        compose.onNodeWithTag("blocked-none").assertDoesNotExist()
        // No token was issued to the peer, so the retry has nothing to revoke.
        click("retry-revocation-1")
        compose.waitForText("blocked-revoke-status", R.string.chats_blocked_not_needed)
        click("unblock-1")
        compose.waitUntil(5_000) { compose.onAllNodesWithTag("blocked-none").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("unblock-1").assertDoesNotExist()
        check(RustBridge.withChats(active()) { it.blockedPeers() }.isEmpty()) { "Unblock was not stored" }
    }

    @Test
    fun theConnectionChipOpensTheNetworkEditorAndSettingsReturn() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        // Other tests leave no active network.
        compose.waitForText("connection-status", R.string.status_no_network)
        compose.onNodeWithTag("connection-status").performClick()
        compose.onNodeWithTag("network-seeds").assertExists()
        compose.onNodeWithTag("skip-network").assertDoesNotExist()
        compose.onNodeWithTag("nav-settings").performClick()
        compose.onNodeWithTag("settings-create").assertExists()
    }
}
