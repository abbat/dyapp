package com.dyapp

import android.app.KeyguardManager
import android.view.WindowManager
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

/** Creates the first identity if this test is the first on a clean device. */
internal fun ComposeTestRule.passIdentityStep() {
    val shown = { tag: String -> onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty() }
    waitUntil(timeoutMillis = 60_000) { shown("app-ready") || shown("skip-network") || shown("create-identity") }
    if (shown("create-identity")) {
        identityAction {
            onNodeWithTag("create-identity").performScrollTo().performClick()
        }
    }
}

/**
 * Waits for the start screen, leaving with Back the network draft that an identity without an
 * active connection opens on; the Argon2id unlock takes seconds on the emulator.
 */
internal fun ComposeTestRule.skipNetworkEntry() {
    passIdentityStep()
    val shown = { tag: String -> onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty() }
    waitUntil(timeoutMillis = 60_000) { shown("app-ready") || shown("skip-network") }
    if (shown("skip-network")) onNodeWithTag("skip-network").performScrollTo().performClick()
}

/** Adds an identity from the switcher sheet in the top bar. */
internal fun ComposeTestRule.addIdentity() {
    onNodeWithTag("identity-switcher").performClick()
    identityAction { onNodeWithTag("add-identity-menu").performClick() }
}

/** Switches to the identity shown as [label] in the switcher sheet. */
internal fun ComposeTestRule.switchIdentity(label: String) {
    onNodeWithTag("identity-switcher").performClick()
    val candidate = hasText(label) and hasAnyAncestor(hasTestTag("identity-menu"))
    awaitNode(candidate)
    identityAction { onNode(candidate).performClick() }
}

/** The ready identity's Peer ID from the Profile section, past the first-run network wizard. */
internal fun ComposeTestRule.shownPeerId(): String {
    skipNetworkEntry()
    if (onAllNodesWithTag("nav-profile").fetchSemanticsNodes().isNotEmpty()) {
        openProfileEditor()
    }
    return onNodeWithTag("peer-id").assertIsDisplayed()
        .fetchSemanticsNode()
        .config[SemanticsProperties.Text]
        .joinToString()
}

class AppLockUITests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private val store = AppLockStore.onDevice(
        InstrumentationRegistry.getInstrumentation().targetContext.noBackupFilesDir,
    )

    private fun string(id: Int) = compose.activity.getString(id)

    private fun enterPin(pin: String) {
        compose.onNodeWithTag("pin-unlock").performTextInput(pin)
        compose.onNodeWithTag("lock-unlock").performClick()
    }

    // The Argon2id check takes seconds on the software-rendered emulator.
    private fun waitForTag(tag: String) = compose.waitUntil(timeoutMillis = 60_000) {
        compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()
    }

    // The API 31 emulator path: FLAG_SECURE keeps the app-switcher snapshot blank.
    private fun secure() =
        compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0

    @Test
    fun configuredLockHidesIdentityUntilTheRightPin() {
        store.enroll("123456")
        try {
            compose.activityRule.scenario.recreate()
            compose.onNodeWithTag("app-locked").assertIsDisplayed()
            check(secure()) { "A configured lock left the snapshot visible" }
            compose.onNodeWithTag("peer-id").assertDoesNotExist()
            compose.onNodeWithTag("recovery-peer").assertDoesNotExist()
            compose.onNodeWithTag("pick-1").assertDoesNotExist()
            // The emulator has no screen lock, so recovery is explained instead of offered.
            if (!compose.activity.getSystemService(KeyguardManager::class.java).isDeviceSecure) {
                compose.onNodeWithTag("lock-device").assertIsNotEnabled()
                compose.onNodeWithTag("lock-recovery-unavailable").assertIsDisplayed()
            }
            enterPin("654321")
            waitForTag("lock-error")
            compose.onNodeWithTag("peer-id").assertDoesNotExist()
            enterPin("123456")
            compose.shownPeerId()
            compose.onNodeWithTag("app-locked").assertDoesNotExist()
        } finally {
            store.resetAfterDeviceCredential(null)
        }
    }

    private fun bothBacksLeaveLockSettings() {
        compose.onNodeWithTag("lock-back").performClick()
        compose.onNodeWithTag("settings-identities").assertExists()
        compose.onNodeWithTag("lock-settings").performScrollTo().performClick()
        // Waits for the screen, whose BackHandler would otherwise miss the press and finish the activity.
        compose.onNodeWithTag("lock-back").assertExists()
        compose.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithTag("settings-identities").assertExists()
    }

    @Test
    fun settingsTurnOffThePinOnlyBehindTheCurrentOneAndKeepTheIdentity() {
        val before = compose.shownPeerId()
        store.enroll("123456")
        try {
            compose.activityRule.scenario.recreate()
            enterPin("123456")
            compose.shownPeerId()
            compose.onNodeWithTag("nav-settings").performClick()
            compose.onNodeWithTag("lock-settings").performScrollTo().performClick()
            compose.onNodeWithTag("lock-mode").assertTextEquals(string(R.string.lock_mode_pin))
            compose.onNodeWithTag("pin-current").performTextInput("654321")
            compose.onNodeWithTag("lock-turn-off").performClick()
            waitForTag("lock-error")
            check(store.mode() == StoredLock.PIN) { "A wrong current PIN turned the lock off" }
            compose.onNodeWithTag("pin-current").performTextInput("123456")
            compose.onNodeWithTag("lock-turn-off").performClick()
            compose.waitUntil(timeoutMillis = 60_000) { store.mode() == StoredLock.OFF }
            compose.onNodeWithTag("lock-mode").assertTextEquals(string(R.string.lock_mode_off))
            check(!secure()) { "Turning the lock off kept FLAG_SECURE" }
            bothBacksLeaveLockSettings()
            check(compose.shownPeerId() == before) { "Turning the lock off changed the identity" }
        } finally {
            runCatching { store.resetAfterDeviceCredential(null) }
        }
    }
}
