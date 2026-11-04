package com.dyapp

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

private const val PUBLISH_WAIT_MS = 30_000L
internal const val READBACK_WAIT_MS = 120_000L

// The core's limit on a profile text field.
private const val PUBLIC_NAME_CHARS = 64

private fun ComposeTestRule.eligibility(id: Int) = waitForText("profile-publish-eligibility", id)

private fun ComposeTestRule.openPublish() {
    onNodeWithTag("profile-publish").performScrollTo().performClick()
}

private fun ComposeTestRule.confirmBoth() {
    onNodeWithTag("profile-attested").performScrollTo().performClick()
    onNodeWithTag("profile-disclosure").performScrollTo().performClick()
    eligibility(R.string.profile_eligible)
}

// Enters a valid year whatever an earlier test saved, and publishes it through the local node.
internal fun ComposeTestRule.publishToReadback() {
    onNodeWithTag("nav-profile").performClick()
    onNodeWithTag("birth-year").performScrollTo().performTextClearance()
    onNodeWithTag("birth-year").performTextInput("1990")
    onNodeWithTag("profile-save").performScrollTo().performClick()
    openPublish()
    confirmBoth()
    onNodeWithTag("profile-publish-confirm").performClick()
    // The software emulator proves and publishes slowly.
    val shown = { tag: String -> onAllNodes(hasTestTag(tag)).fetchSemanticsNodes().isNotEmpty() }
    // A failed publish waits in the dialog, so stop there and say which step never ended.
    runCatching {
        waitUntil(READBACK_WAIT_MS) { shown("profile-published-readback") || shown("profile-publish-error") }
    }.onFailure {
        val state = "publishing step: ${shown("profile-publish-status")}, " +
            "dialog: ${shown("profile-publish-confirm")}, proof: ${texts("profile-proof-progress")}"
        throw AssertionError("Stuck, $state", it)
    }
    check(!shown("profile-publish-error")) { "Publish failed: ${texts("profile-publish-error")}" }
}

class ProfilePublishTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun hiddenNotices() = compose.onAllNodes(hasTestTag("profile-hidden-notice")).fetchSemanticsNodes()

    // Closing the dialog drops its confirmations.
    private fun cancelDropsConfirmations() {
        compose.openPublish()
        compose.onNodeWithTag("profile-publish-confirm").assertIsNotEnabled()
        compose.confirmBoth()
        compose.onNodeWithTag("profile-cancel").performClick()
        compose.openPublish()
        compose.onNodeWithTag("profile-attested").performScrollTo().assertIsOff()
        compose.onNodeWithTag("profile-cancel").performClick()
    }

    private fun unsavedEditBlocksPublishing() {
        // Saving an empty year first makes the next input an edit whatever an earlier test saved.
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()

        compose.onNodeWithTag("birth-year").performScrollTo().performTextInput("2000")
        compose.eligibility(R.string.profile_save_changes_before_publish)
        compose.onNodeWithTag("profile-publish").performScrollTo().assertIsNotEnabled()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-publish").performScrollTo().assertIsEnabled()
    }

    private fun saveName(name: String, status: Int) {
        compose.onNodeWithTag("profile-name").performScrollTo().performTextReplacement(name)
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(status))
    }

    @Test
    fun aPublicNameIsTrimmedOnSaveAndARefusedOneSaysWhy() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        saveName("x".repeat(PUBLIC_NAME_CHARS + 1), R.string.profile_name_invalid)
        saveName("  Anna  ", R.string.profile_saved)
        compose.openPublish()
        val row = compose.activity.getString(R.string.profile_preview_row, string(R.string.profile_name_row), "Anna")
        compose.onNodeWithTag("preview-name").performScrollTo().assertTextEquals(row)
        compose.onNodeWithTag("profile-cancel").performClick()
        // Later tests start without a public name.
        saveName("", R.string.profile_saved)
    }

    private fun saveHeight(height: String, status: Int) {
        compose.onNodeWithTag("profile-height").performScrollTo().performTextReplacement(height)
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(status))
    }

    @Test
    fun aHeightIsShownClearedAndARefusedOneSaysWhy() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        saveHeight("49", R.string.profile_height_invalid)
        saveHeight("tall", R.string.profile_height_invalid)
        saveHeight("180", R.string.profile_saved)
        compose.openPublish()
        val row = compose.activity.getString(R.string.profile_preview_row, string(R.string.profile_height), "180")
        compose.onNodeWithText(row).performScrollTo().assertExists()
        compose.onNodeWithTag("profile-cancel").performClick()
        // Later tests start without a height.
        saveHeight("", R.string.profile_saved)
    }

    private fun saveAbout(about: String, status: Int) {
        compose.onNodeWithTag("profile-about").performScrollTo().performTextReplacement(about)
        // A touch can miss: the keyboard raised by the field may still be moving the form.
        compose.onNodeWithTag("profile-save").performSemanticsAction(SemanticsActions.OnClick)
        compose.onNodeWithTag("profile-status").assertTextEquals(string(status))
    }

    @Test
    fun aboutIsShownAsPlainTextClearedAndARefusedOneSaysWhy() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        saveAbout("x".repeat(501), R.string.profile_about_invalid)
        saveAbout("An${Char(0x202E)}na", R.string.profile_about_invalid)
        val about = "First line\n<b>not bold</b>"
        saveAbout(about, R.string.profile_saved)
        compose.openPublish()
        val row = compose.activity.getString(R.string.profile_preview_row, string(R.string.profile_about), about)
        compose.onNodeWithTag("preview-about").performScrollTo().assertTextEquals(row)
        compose.onNodeWithTag("profile-cancel").performClick()
        // Later tests start without about.
        saveAbout("", R.string.profile_saved)
    }

    @Test
    fun publishNeedsFreshConfirmationsAndReportsNoNetwork() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        cancelDropsConfirmations()
        unsavedEditBlocksPublishing()
        compose.openPublish()
        compose.onNodeWithTag("profile-attested").performScrollTo().assertIsOff()
        compose.confirmBoth()

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
            compose.publishToReadback()
            compose.onNodeWithTag("profile-publish-error").assertDoesNotExist()
            compose.onNodeWithText(string(R.string.profile_published)).performScrollTo().assertIsDisplayed()
            compose.onNodeWithTag("profile-publish-confirm").assertDoesNotExist()
            compose.openPublish()
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
            compose.publishToReadback()
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsEnabled().assertIsOff().performClick()
            compose.waitUntil(READBACK_WAIT_MS) { hiddenNotices().isNotEmpty() }
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOn()

            // Off opens the publish dialog; cancelling it leaves the profile hidden.
            compose.onNodeWithTag("profile-hidden").performClick()
            compose.onNodeWithTag("profile-cancel").performClick()
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOn()

            compose.onNodeWithTag("profile-hidden").performClick()
            compose.confirmBoth()
            compose.onNodeWithTag("profile-publish-confirm").performClick()
            compose.waitUntil(READBACK_WAIT_MS) { hiddenNotices().isEmpty() }
            compose.onNodeWithTag("profile-hidden").performScrollTo().assertIsOff()
        } finally {
            forgetNetworks()
        }
    }
}
