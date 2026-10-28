package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
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

    private fun texts(tag: String) = compose.onAllNodes(hasTestTag(tag)).fetchSemanticsNodes()
        .map { it.config.getOrNull(SemanticsProperties.Text)?.joinToString() }

    // A timeout names what the tag and the network check rows showed instead.
    private fun waitForText(tag: String, id: Int) = runCatching {
        compose.waitUntil(PUBLISH_WAIT_MS) {
            compose.onAllNodes(hasTestTag(tag) and hasText(string(id))).fetchSemanticsNodes().isNotEmpty()
        }
    }.onFailure { throw AssertionError("$tag shows ${texts(tag)}, rows ${texts("network-row")}", it) }

    private fun eligibility(id: Int) = waitForText("profile-publish-eligibility", id)

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
        compose.onNodeWithTag("network-check").performScrollTo().performClick()
        waitForText("network-check-status", R.string.network_connected)
        compose.onNodeWithTag("network-continue").performScrollTo().performClick()
        waitForText("network-check-status", R.string.network_active)
        compose.onNodeWithTag("network-back").performScrollTo().performClick()
    }

    // Other tests expect no active network, so the activated settings never outlive this test.
    private fun forgetNetworks() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        for (root in listOf(context.filesDir, context.noBackupFilesDir)) {
            root.walkBottomUp().filter { it.isFile && it.name in NETWORK_FILES }.forEach { it.delete() }
        }
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

        // The emulator runner's loopback dyappd, as the guest reaches its container.
        const val LOCAL_NODE = "10.0.2.2:33445"
        val NETWORK_FILES = setOf("network", "network-draft")
    }
}
