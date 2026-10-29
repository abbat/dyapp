package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
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

    private fun confirmBoth() {
        compose.onNodeWithTag("profile-confirm-adult").performScrollTo().performClick()
        compose.onNodeWithTag("profile-confirm-public").performScrollTo().performClick()
        eligibility(R.string.profile_eligible)
    }

    @Test
    fun publishNeedsFreshConfirmationsAndReportsNoNetwork() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("profile-edit").performScrollTo().performClick()
        compose.onNodeWithTag("profile-prepare-publish").performScrollTo().assertIsNotEnabled()
        confirmBoth()

        // An unsaved edit clears and disables both confirmations until it is saved.
        compose.onNodeWithTag("profile-birth-year").performScrollTo().performTextInput("2000")
        eligibility(R.string.profile_save_changes_before_publish)
        compose.onNodeWithTag("profile-confirm-adult").performScrollTo().assertIsOff().assertIsNotEnabled()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-confirm-adult").performScrollTo().assertIsOff().assertIsEnabled()
        confirmBoth()

        // Skipped network entry leaves no active network: the proof runs, publication fails and retries.
        compose.onNodeWithTag("profile-prepare-publish").performScrollTo().performClick()
        compose.waitUntil(PUBLISH_WAIT_MS) {
            compose.onAllNodes(hasTestTag("profile-publish-error")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("profile-publish-error").performScrollTo()
            .assertTextEquals(string(R.string.profile_publish_error_not_connected))
        compose.onNodeWithTag("profile-confirm-public").performScrollTo().assertIsOff()
        compose.onNodeWithText(string(R.string.profile_retry_publish)).assertExists()
        compose.onNodeWithTag("profile-prepare-publish").assertIsNotEnabled()
    }

    private fun activateLocalNode() {
        compose.onNodeWithTag("network-draft-open").performScrollTo().performClick()
        compose.onNodeWithTag("network-seeds").performScrollTo().performTextClearance()
        compose.onNodeWithTag("network-seeds").performTextInput(LOCAL_NODE)
        compose.checkNetwork(retryUnreachable = true)
        compose.waitForText("network-check-status", R.string.network_connected)
        compose.onNodeWithTag("network-continue").performScrollTo().performClick()
        compose.waitForText("network-check-status", R.string.network_active)
        compose.onNodeWithTag("network-back").performScrollTo().performClick()
    }

    @Test
    fun publishThroughTheLocalNodeShowsTheSignedReadback() {
        compose.skipNetworkEntry()
        try {
            activateLocalNode()
            compose.onNodeWithTag("profile-edit").performScrollTo().performClick()
            compose.onNodeWithTag("profile-birth-year").performScrollTo().performTextClearance()
            compose.onNodeWithTag("profile-birth-year").performTextInput("1990")
            compose.onNodeWithTag("profile-save").performScrollTo().performClick()
            confirmBoth()
            compose.onNodeWithTag("profile-prepare-publish").performScrollTo().performClick()
            // The software emulator proves and publishes slowly.
            compose.waitUntil(READBACK_WAIT_MS) {
                compose.onAllNodes(hasTestTag("profile-published-readback")).fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithTag("profile-publish-error").assertDoesNotExist()
            compose.onNodeWithText(string(R.string.profile_published)).performScrollTo().assertIsDisplayed()
            compose.onNodeWithTag("profile-confirm-adult").performScrollTo().assertIsOff()
        } finally {
            forgetNetworks()
        }
    }

    private companion object {
        const val PUBLISH_WAIT_MS = 30_000L
        const val READBACK_WAIT_MS = 120_000L
    }
}
