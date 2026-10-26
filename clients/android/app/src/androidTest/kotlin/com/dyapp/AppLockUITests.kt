package com.dyapp

import android.view.WindowManager
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsNodeInteractionsProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

internal fun SemanticsNodeInteractionsProvider.shownPeerId() =
    onNodeWithTag("peer-id").assertIsDisplayed()
        .fetchSemanticsNode()
        .config[SemanticsProperties.Text]
        .joinToString()

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
        compose.onNodeWithTag("pin").performTextInput(pin)
        compose.onNodeWithTag("pin-unlock").performClick()
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
            compose.onNodeWithTag("identity").assertDoesNotExist()
            enterPin("654321")
            waitForTag("pin-error")
            compose.onNodeWithTag("peer-id").assertDoesNotExist()
            enterPin("123456")
            waitForTag("peer-id")
            compose.onNodeWithTag("app-locked").assertDoesNotExist()
        } finally {
            store.resetAfterDeviceCredential(null)
        }
    }

    @Test
    fun settingsTurnOffThePinOnlyBehindTheCurrentOneAndKeepTheIdentity() {
        val before = compose.shownPeerId()
        store.enroll("123456")
        try {
            compose.activityRule.scenario.recreate()
            enterPin("123456")
            waitForTag("peer-id")
            compose.onNodeWithTag("lock-settings").performScrollTo().performClick()
            compose.onNodeWithTag("lock-mode").assertTextEquals(string(R.string.lock_mode_pin))
            compose.onNodeWithTag("pin-current").performTextInput("654321")
            compose.onNodeWithTag("lock-off").performClick()
            waitForTag("pin-error")
            check(store.hasPin()) { "A wrong current PIN turned the lock off" }
            compose.onNodeWithTag("pin-current").performTextInput("123456")
            compose.onNodeWithTag("lock-off").performClick()
            compose.waitUntil(timeoutMillis = 60_000) { !store.hasPin() }
            compose.onNodeWithTag("lock-mode").assertTextEquals(string(R.string.lock_mode_off))
            check(!secure()) { "Turning the lock off kept FLAG_SECURE" }
            compose.onNodeWithTag("lock-back").performClick()
            check(compose.shownPeerId() == before) { "Turning the lock off changed the identity" }
        } finally {
            runCatching { store.resetAfterDeviceCredential(null) }
        }
    }
}
