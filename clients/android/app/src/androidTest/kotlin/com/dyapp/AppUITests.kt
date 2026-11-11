package com.dyapp

import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import java.io.File

class AppUITests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    @Test
    fun launchesAndRendersReady() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
            .assertTextEquals(string(R.string.nav_profile))
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
        // A network wizard closed with Back stays closed.
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
            .assertTextEquals(string(R.string.nav_profile))
        // The new screen unwraps the stored key again, as a restarted app does.
        check(compose.shownPeerId() == before) { "Peer ID changed on recreation" }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val opened = RustBridge.slots(context).open() as Opened.Ready
        check(opened.peerId == before) { "Stored identity differs from the shown one" }
        check(File(opened.slot.dir, "identity.key").length() > 32) { "Key not wrapped" }
        check(PlatformKeyStore.exists(identityAlias(opened.slot.id))) { "No wrapping key for the slot" }
    }

    @Test
    fun addSwitchAndRelaunchKeepTheChosenIdentity() {
        val first = compose.shownPeerId()
        compose.addIdentity()
        val second = compose.shownPeerId()
        check(second != first) { "A new identity reused the Peer ID" }
        compose.onNodeWithTag("identity-switcher").performClick()
        compose.awaitNode(hasText(first) and hasAnyAncestor(hasTestTag("identity-menu"))).assertExists()
        compose.onNode(hasText(second) and hasAnyAncestor(hasTestTag("identity-menu"))).assertExists()
        compose.onNodeWithTag("add-identity-menu").assertExists()
        compose.identityAction {
            compose.onNode(hasText(first) and hasAnyAncestor(hasTestTag("identity-menu"))).performClick()
        }
        check(compose.shownPeerId() == first) { "Switching did not open the chosen identity" }
        // A relaunch reads the active pointer again.
        compose.activityRule.scenario.recreate()
        check(compose.shownPeerId() == first) { "The chosen identity was lost on relaunch" }
    }

    @Test
    fun removingTheActiveIdentityErasesItsKeyAndReturnsToThePicker() {
        val first = compose.shownPeerId()
        compose.addIdentity()
        val second = compose.shownPeerId()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alias = identityAlias((RustBridge.slots(context).open() as Opened.Ready).slot.id)
        compose.onNodeWithTag("nav-settings").performClick()
        compose.awaitNode(hasTestTag("settings-remove")).performScrollTo().performClick()
        compose.onNodeWithTag("remove-title").assertTextEquals(string(R.string.remove_title))
        compose.onNodeWithTag("remove-peer").assertTextEquals(second)
        compose.onNodeWithText(string(R.string.remove_last_copy)).assertIsDisplayed()
        compose.onNodeWithTag("remove-warning").assertIsDisplayed()
        compose.onNodeWithTag("remove-action").assertIsNotEnabled()
        compose.onNodeWithTag("remove-confirm").performScrollTo().performClick()
        compose.identityAction { compose.onNodeWithTag("remove-action").performClick() }
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_pick))
        compose.onNodeWithText(shortPeerId(second)).assertDoesNotExist()
        check(!PlatformKeyStore.exists(alias)) { "The removed identity kept its key" }
        compose.identityAction { compose.onNodeWithText(shortPeerId(first)).performScrollTo().performClick() }
        check(compose.shownPeerId() == first) { "The remaining identity did not open" }
    }

    private fun copiesRecoveryDetails(context: Context) {
        compose.onNodeWithText(string(R.string.details)).performClick()
        compose.onNodeWithTag("recovery-details").assertTextEquals("incomplete")
        compose.onNodeWithTag("copy-recovery-details").performClick()
        val clip = context.getSystemService(ClipboardManager::class.java).primaryClip
        check(clip?.getItemAt(0)?.text == "incomplete") { "Copy did not reach the clipboard" }
    }

    @Test
    fun interruptedSaveIsRecoveryNotANewIdentity() {
        compose.shownPeerId()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val slots = RustBridge.slots(context)
        val active = (slots.open() as Opened.Ready).slot.id
        // A crash after the slot was reserved: `pending` and no key yet.
        val id = "00000000-0000-4000-8000-000000000000"
        val dir = File(context.noBackupFilesDir, "profiles/$id")
        try {
            check(dir.mkdirs())
            File(dir, "state").writeText("pending\n")
            val incomplete = string(R.string.recovery_incomplete)
            compose.switchIdentity(incomplete)
            compose.onNodeWithTag("recovery-reason").assertTextEquals(incomplete)
            compose.identityAction { compose.onNodeWithTag("retry").performClick() }
            compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.status_recovery))
            check(!PlatformKeyStore.exists("dyapp-identity-wrap:$id")) { "Retry made a key" }
            check((slots.open() as Opened.Ready).slot.id == active) { "Recovery changed the saved selection" }
            copiesRecoveryDetails(context)
            // No key exists, so removing loses nothing and shows no loss warning.
            compose.onNodeWithTag("remove-identity").performScrollTo().performClick()
            compose.onNodeWithTag("remove-confirm").assertExists()
            compose.onNodeWithTag("remove-warning").assertDoesNotExist()
            compose.onNodeWithTag("remove-cancel").performClick()
        } finally {
            dir.deleteRecursively()
            slots.choose(active)
        }
    }
}
