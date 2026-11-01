package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

class ProfilePublishTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun eligibility(id: Int) = compose.waitForText("profile-publish-eligibility", id)

    private fun openPublish() {
        compose.onNodeWithTag("profile-publish").performScrollTo().performClick()
    }

    private fun confirmBoth() {
        compose.onNodeWithTag("profile-attested").performScrollTo().performClick()
        compose.onNodeWithTag("profile-disclosure").performScrollTo().performClick()
        eligibility(R.string.profile_eligible)
    }

    // Enters a valid year whatever an earlier test saved, and publishes it through the local node.
    private fun publishToReadback() {
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("birth-year").performTextInput("1990")
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        openPublish()
        confirmBoth()
        compose.onNodeWithTag("profile-publish-confirm").performClick()
        // The software emulator proves and publishes slowly.
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(hasTestTag("profile-published-readback")).fetchSemanticsNodes().isNotEmpty()
        }
    }

    private fun hiddenNotices() = compose.onAllNodes(hasTestTag("profile-hidden-notice")).fetchSemanticsNodes()

    @Test
    fun publishNeedsFreshConfirmationsAndReportsNoNetwork() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        openPublish()
        compose.onNodeWithTag("profile-publish-confirm").assertIsNotEnabled()
        confirmBoth()

        // Closing the dialog drops its confirmations.
        compose.onNodeWithTag("profile-cancel").performClick()
        openPublish()
        compose.onNodeWithTag("profile-attested").performScrollTo().assertIsOff()
        compose.onNodeWithTag("profile-cancel").performClick()

        // Saving an empty year first makes the next input an edit whatever an earlier test saved.
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()

        // An unsaved edit disables publishing until it is saved.
        compose.onNodeWithTag("birth-year").performScrollTo().performTextInput("2000")
        eligibility(R.string.profile_save_changes_before_publish)
        compose.onNodeWithTag("profile-publish").performScrollTo().assertIsNotEnabled()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-publish").performScrollTo().assertIsEnabled()
        openPublish()
        compose.onNodeWithTag("profile-attested").performScrollTo().assertIsOff()
        confirmBoth()

        // Skipped network entry leaves no active network: the proof runs, publication fails and retries.
        compose.onNodeWithTag("profile-publish-confirm").performClick()
        compose.waitUntil(PUBLISH_WAIT_MS) {
            compose.onAllNodes(hasTestTag("profile-publish-error")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("profile-publish-error").performScrollTo()
            .assertTextEquals(string(R.string.profile_publish_error_not_connected))
        compose.onNodeWithTag("profile-disclosure").performScrollTo().assertIsOff()
        compose.onNodeWithText(string(R.string.profile_retry_publish)).assertExists()
        compose.onNodeWithTag("profile-publish-confirm").assertIsNotEnabled()
    }

    @Test
    fun publishThroughTheLocalNodeShowsTheSignedReadback() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            publishToReadback()
            compose.onNodeWithTag("profile-publish-error").assertDoesNotExist()
            compose.onNodeWithText(string(R.string.profile_published)).performScrollTo().assertIsDisplayed()
            compose.onNodeWithTag("profile-publish-confirm").assertDoesNotExist()
            openPublish()
            compose.onNodeWithTag("profile-attested").performScrollTo().assertIsOff()
        } finally {
            forgetNetworks()
        }
    }

    @Test
    fun theSwitchHidesAndOnlyAPublicationShowsAgain() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            publishToReadback()
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsEnabled().assertIsOff().performClick()
            compose.waitUntil(READBACK_WAIT_MS) { hiddenNotices().isNotEmpty() }
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOn()

            // Off opens the publish dialog; cancelling it leaves the profile hidden.
            compose.onNodeWithTag("profile-hidden").performClick()
            compose.onNodeWithTag("profile-cancel").performClick()
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOn()

            compose.onNodeWithTag("profile-hidden").performClick()
            confirmBoth()
            compose.onNodeWithTag("profile-publish-confirm").performClick()
            compose.waitUntil(READBACK_WAIT_MS) { hiddenNotices().isEmpty() }
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOff()
        } finally {
            forgetNetworks()
        }
    }

    // A second identity, so that removing it leaves the first one for the next test.
    private fun publishSecondIdentity(): String {
        val first = compose.shownPeerId()
        compose.addIdentity()
        compose.shownPeerId()
        compose.activateLocalNode()
        publishToReadback()
        return first
    }

    private fun activeAlias(): String {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return (RustBridge.slots(context).open() as Opened.Ready).slot.alias
    }

    private fun askToRemove() {
        compose.onNodeWithTag("nav-settings").performClick()
        compose.onNodeWithTag("settings-remove").performScrollTo().performClick()
        compose.onNodeWithText(string(R.string.remove_withdraws)).assertExists()
        compose.onNodeWithTag("remove-confirm").performScrollTo().performClick()
        compose.onNodeWithTag("remove-action").performClick()
    }

    private fun awaitOffline() = compose.waitUntil(READBACK_WAIT_MS) {
        compose.onAllNodes(hasTestTag("remove-retry") and isEnabled()).fetchSemanticsNodes().isNotEmpty()
    }

    private fun assertRemoved(alias: String, first: String) {
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(hasTestTag("remove-title")).fetchSemanticsNodes().isEmpty()
        }
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_pick))
        check(!PlatformKeyStore.exists(alias)) { "The removed identity kept its key" }
        compose.onNodeWithText(shortPeerId(first)).performScrollTo().performClick()
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

    private companion object {
        const val PUBLISH_WAIT_MS = 30_000L
        const val READBACK_WAIT_MS = 120_000L
    }
}
