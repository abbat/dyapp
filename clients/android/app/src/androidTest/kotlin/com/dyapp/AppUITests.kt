package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.time.LocalDate
import java.time.ZoneOffset
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

class AppUITests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    @Test
    fun launchesAndRendersReady() {
        compose.skipNetworkEntry()
        compose.onNodeWithText(string(R.string.app_name)).assertIsDisplayed()
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
            .assertTextEquals(string(R.string.status_ready))
        // The peer ID comes from the Rust core through the UniFFI bindings.
        val peerId = compose.shownPeerId()
        check(Regex("[0-9a-f]{64}").matches(peerId)) { "Unexpected peer ID: $peerId" }
        compose.onNodeWithTag("os-loss").assertTextEquals(string(R.string.storage_os_loss))
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val ownsActiveWindow = {
            instrumentation.uiAutomation.rootInActiveWindow?.packageName?.toString() ==
                instrumentation.targetContext.packageName
        }
        // Window focus moves to the activity asynchronously, after Compose is already idle.
        runCatching { compose.waitUntil(timeoutMillis = 10_000) { ownsActiveWindow() } }
        check(ownsActiveWindow()) { "Application UI is obscured by another active window" }
        // The app's own window via PixelCopy: UiAutomation.takeScreenshot() returns null on the
        // software-rendered API 31 emulator.
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        File(instrumentation.targetContext.filesDir, "ui-ready.png").outputStream().use {
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
        }
    }

    @Test
    fun recreationKeepsReadyScreenAndPeerId() {
        val before = compose.shownPeerId()
        compose.activityRule.scenario.recreate()
        // A network draft closed with Back stays closed.
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
            .assertTextEquals(string(R.string.status_ready))
        // The new screen unwraps the stored key again, as a restarted app does.
        check(compose.shownPeerId() == before) { "Peer ID changed on recreation" }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val opened = RustBridge.slots(context).open() as Opened.Ready
        check(opened.peerId == before) { "Stored identity differs from the shown one" }
        check(opened.slot.keyFile.length() > 32) { "Key not wrapped" }
        check(PlatformKeyStore.exists(opened.slot.alias)) { "No wrapping key for the slot" }
    }

    @Test
    fun addSwitchAndRelaunchKeepTheChosenIdentity() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("switch").performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_pick))
        compose.onNodeWithTag("add-identity").performScrollTo().performClick()
        val second = compose.shownPeerId()
        check(second != first) { "A new identity reused the Peer ID" }
        compose.onNodeWithTag("switch").performClick()
        compose.onNode(hasTestTag("identity") and hasText(second)).performScrollTo()
        compose.onNode(hasTestTag("identity") and hasText(first)).performScrollTo().performClick()
        check(compose.shownPeerId() == first) { "Switching did not open the chosen identity" }
        // A relaunch reads the active pointer again.
        compose.activityRule.scenario.recreate()
        check(compose.shownPeerId() == first) { "The chosen identity was lost on relaunch" }
    }

    @Test
    fun removingTheActiveIdentityErasesItsKeyAndReturnsToThePicker() {
        val first = compose.shownPeerId()
        compose.onNodeWithTag("switch").performClick()
        compose.onNodeWithTag("add-identity").performScrollTo().performClick()
        val second = compose.shownPeerId()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alias = (RustBridge.slots(context).open() as Opened.Ready).slot.alias
        compose.onNodeWithTag("remove-identity").performScrollTo().performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.remove_title))
        compose.onNodeWithText(string(R.string.remove_last_copy)).assertIsDisplayed()
        compose.onNodeWithTag("remove-warning").assertIsDisplayed()
        compose.onNodeWithTag("remove-action").assertIsNotEnabled()
        compose.onNodeWithTag("remove-confirm").performScrollTo().performClick()
        compose.onNodeWithTag("remove-action").performScrollTo().performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_pick))
        compose.onNode(hasTestTag("identity") and hasText(second)).assertDoesNotExist()
        check(!PlatformKeyStore.exists(alias)) { "The removed identity kept its key" }
        compose.onNode(hasTestTag("identity") and hasText(first)).performScrollTo().performClick()
        check(compose.shownPeerId() == first) { "The remaining identity did not open" }
    }

    @Test
    fun previewShowsOnlyTheSavedDraftWithAgeAndWarnings() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("profile-edit").performScrollTo().performClick()
        val parts = listOf("profile-birth-year" to "2000", "profile-birth-month" to "1", "profile-birth-day" to "1")
        for ((tag, value) in parts) {
            compose.onNodeWithTag(tag).performScrollTo().performTextClearance()
            compose.onNodeWithTag(tag).performTextInput(value)
        }
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        // Born on 1 January, so the age is exact on any day of the year.
        val age = LocalDate.now(ZoneOffset.UTC).year - 2000
        compose.onNodeWithTag("profile-preview-age").performScrollTo()
            .assertTextEquals(compose.activity.getString(R.string.profile_age, "$age"))
        compose.onNodeWithTag("profile-public-disclosure").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("profile-full-date").performScrollTo().assertIsDisplayed()

        // An unsaved edit does not change the preview; a saved one does.
        compose.onNodeWithTag("profile-birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        compose.onNodeWithTag("profile-full-date").assertExists()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        val unknown = compose.activity.getString(R.string.profile_age, string(R.string.profile_age_unknown))
        compose.onNodeWithTag("profile-preview-age").performScrollTo().assertTextEquals(unknown)
        compose.onNodeWithTag("profile-full-date").assertDoesNotExist()
    }

    @Test
    fun interruptedSaveIsRecoveryNotANewIdentity() {
        compose.shownPeerId()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val slots = RustBridge.slots(context)
        val active = checkNotNull(slots.active())
        // A crash after the slot was reserved: `pending` and no key yet.
        val id = "00000000-0000-4000-8000-000000000000"
        val dir = File(context.noBackupFilesDir, "profiles/$id")
        try {
            check(dir.mkdirs())
            File(dir, "state").writeText("pending\n")
            val incomplete = string(R.string.recovery_incomplete)
            compose.onNodeWithTag("switch").performClick()
            compose.onNode(hasTestTag("identity") and hasText(incomplete)).performScrollTo()
                .performClick()
            compose.onNodeWithTag("recovery-reason").assertTextEquals(incomplete)
            compose.onNodeWithTag("retry").performClick()
            compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_recovery))
            check(!PlatformKeyStore.exists("dyapp-identity-wrap:$id")) { "Retry made a key" }
            // No key exists, so removing loses nothing and shows no loss warning.
            compose.onNodeWithTag("remove-identity").performScrollTo().performClick()
            compose.onNodeWithTag("remove-confirm").assertExists()
            compose.onNodeWithTag("remove-warning").assertDoesNotExist()
            compose.onNodeWithTag("remove-cancel").performScrollTo().performClick()
        } finally {
            dir.deleteRecursively()
            slots.setActive(active)
        }
    }
}
