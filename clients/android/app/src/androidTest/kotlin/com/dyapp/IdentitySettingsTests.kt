package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.ChatNotice

class IdentitySettingsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val slots = RustBridge.slots(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun active() = slots.open() as Opened.Ready

    // The settings lists load after the screen opens, so a row may not exist yet.
    private fun click(tag: String) {
        compose.awaitNode(hasTestTag(tag) and isEnabled()).performScrollTo().performClick()
    }

    @Test
    fun createSwitchAndConfirmedRemoveChangeTheActiveIdentity() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val rows = compose.identityRows()
        compose.identityAction { click("settings-create") }
        val second = active()
        check(second.peerId != first) { "Create kept the previous identity active" }
        compose.identityRows(rows + 1)
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
        compose.awaitNode(hasTestTag("remove-action")).assertIsNotEnabled()
        compose.onNodeWithTag("remove-cancel").performClick()
        compose.onNodeWithTag("settings-create").assertExists()
        check(active().peerId == second.peerId) { "Cancel changed the active identity" }
        click("settings-remove")
        click("remove-confirm")
        compose.identityAction { compose.onNodeWithTag("remove-action").performClick() }
        compose.onNodeWithTag("app-ready").assertTextEquals(compose.activity.getString(R.string.status_pick))
        check(!PlatformKeyStore.exists(identityAlias(second.slot.id))) { "The removed identity kept its key" }
        // Later tests start from the active identity, not the picker.
        compose.identityAction { compose.onNodeWithText(shortPeerId(first)).performScrollTo().performClick() }
        check(compose.shownPeerId() == first) { "The remaining identity did not open" }
    }

    @Test
    fun removingAnotherIdentityStaysInSettings() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val rows = compose.identityRows()
        compose.identityAction { click("settings-create") }
        val other = active()
        compose.switchIdentity(first)
        compose.identityRows(rows + 1)
        compose.onNode(
            hasText(compose.activity.getString(R.string.identity_remove)) and hasAnyAncestor(
                hasTestTag("settings-identity") and hasAnyDescendant(hasText(shortPeerId(other.peerId)))
            )
        ).performScrollTo().performClick()
        click("remove-confirm")
        compose.identityAction { compose.onNodeWithTag("remove-action").performClick() }
        compose.onNodeWithTag("settings-create").assertExists()
        check(active().peerId == first) { "Removing another identity changed the active one" }
        check(!PlatformKeyStore.exists(identityAlias(other.slot.id))) { "The removed identity kept its key" }
        compose.identityRows(rows)
    }

    @Test
    fun theNoticeSwitchIsStoredPerIdentity() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        val switch = compose.onNodeWithTag("message-notifications").performScrollTo()

        // On by default; the emulator allows notices, so nothing asks for them.
        waitNotice(true)
        compose.onNodeWithTag("notice-blocked").assertDoesNotExist()
        compose.onNodeWithTag("notice-fixed").assertIsSelected()
        click("notice-name")
        compose.waitUntil(5_000) { stored() == ChatNotice.NAME }
        click("notice-text")
        compose.waitUntil(5_000) { stored() == ChatNotice.TEXT }
        waitNotice(true)
        switch.performScrollTo().performClick()
        waitNotice(false)
        check(stored() == ChatNotice.OFF) { "Off was not stored" }
        compose.activityRule.scenario.recreate()
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-settings").performClick()
        waitNotice(false)
        // Off keeps the choice, disabled, and a turn-on brings it back.
        compose.onNodeWithTag("notice-text").performScrollTo().assertIsSelected().assertIsNotEnabled()
        switch.performScrollTo().performClick()
        waitNotice(true)
        check(stored() == ChatNotice.TEXT) { "On lost the content choice" }
        click("notice-fixed")
        compose.waitUntil(5_000) { stored() == ChatNotice.FIXED }
    }

    // Enabled semantics prove native load/save completion; off alone could still mean loading.
    private fun waitNotice(on: Boolean) {
        val node = compose.awaitNode(hasTestTag("message-notifications") and isEnabled())
        if (on) node.assertIsOn() else node.assertIsOff()
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
