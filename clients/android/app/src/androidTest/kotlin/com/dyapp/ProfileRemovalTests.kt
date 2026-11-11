package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class ProfileRemovalTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private var removalRevision = 0L

    private fun string(id: Int) = compose.activity.getString(id)

    // A second identity, so that removing it leaves the first one for the next test.
    private fun publishSecondIdentity(): String {
        val first = compose.shownPeerId()
        compose.addIdentity()
        compose.shownPeerId()
        compose.activateLocalNode()
        compose.publishToReadback()
        return first
    }

    private fun activeAlias(): String {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return identityAlias((RustBridge.slots(context).open() as Opened.Ready).slot.id)
    }

    private fun askToRemove() {
        compose.onNodeWithTag("nav-settings").performClick()
        compose.awaitNode(hasTestTag("settings-remove")).performScrollTo().performClick()
        compose.onNodeWithText(string(R.string.remove_withdraws)).assertExists()
        compose.onNodeWithTag("remove-confirm").performScrollTo().performClick()
        removalRevision = compose.identityRevision()
        compose.onNodeWithTag("remove-action").performClick()
    }

    private fun awaitOffline() = compose.waitUntil(READBACK_WAIT_MS) {
        compose.onAllNodes(hasTestTag("remove-retry") and isEnabled()).fetchSemanticsNodes().isNotEmpty()
    }

    private fun assertRemoved(alias: String, first: String) {
        compose.awaitIdentityCompletion(removalRevision, READBACK_WAIT_MS)
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(hasTestTag("remove-title")).fetchSemanticsNodes().isEmpty()
        }
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_pick))
        check(!PlatformKeyStore.exists(alias)) { "The removed identity kept its key" }
        compose.identityAction { compose.onNodeWithText(shortPeerId(first)).performScrollTo().performClick() }
    }

    @Test
    fun removalWithdrawsThePublishedProfileAndWaitsForANode() {
        compose.skipNetworkEntry()
        try {
            val first = publishSecondIdentity()
            val alias = activeAlias()
            forgetNetworks()
            askToRemove()
            awaitOffline()
            compose.onNodeWithTag("remove-offline").assertExists()

            // Retry finds no node either, and Cancel keeps the identity.
            compose.onNodeWithTag("remove-retry").performClick()
            compose.waitForIdle()
            awaitOffline()
            compose.onNodeWithTag("remove-cancel-offline").performClick()
            check(PlatformKeyStore.exists(alias)) { "Cancel removed the identity" }

            compose.activateLocalNode()
            askToRemove()
            assertRemoved(alias, first)
        } finally {
            forgetNetworks()
        }
    }

    @Test
    fun removeAnywayRemovesWithoutANode() {
        compose.skipNetworkEntry()
        try {
            val first = publishSecondIdentity()
            val alias = activeAlias()
            forgetNetworks()
            askToRemove()
            awaitOffline()
            compose.onNodeWithTag("remove-anyway").performClick()
            assertRemoved(alias, first)
        } finally {
            forgetNetworks()
        }
    }
}
