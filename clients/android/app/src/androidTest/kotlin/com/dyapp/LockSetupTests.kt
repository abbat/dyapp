package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExternalResource
import org.junit.rules.RuleChain

/** Sets or clears the first-launch Skip marker before the activity starts. */
fun lockChoice(skipped: Boolean) = object : ExternalResource() {
    override fun before() {
        val root = InstrumentationRegistry.getInstrumentation().targetContext.noBackupFilesDir
        val marker = File(root, AppLockStore.SKIPPED)
        if (skipped) marker.createNewFile() else marker.delete()
    }
}

class LockSetupTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = false)).around(compose)

    @Test
    fun firstLaunchOffersSetupBeforeIdentityAndRemembersSkip() {
        compose.onNodeWithTag("lock-setup").assertIsDisplayed()
        compose.onNodeWithTag("peer-id").assertDoesNotExist()
        compose.onNodeWithTag("lock-skip").performClick()
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
        compose.activityRule.scenario.recreate()
        compose.onNodeWithTag("lock-setup").assertDoesNotExist()
        compose.onNodeWithTag("app-ready").assertIsDisplayed()
    }
}

class SetupScreenTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun mismatchClearsInputAndMatchingPinEnrollsOnce() {
        val enrolled = mutableListOf<String>()
        compose.setContent { SetupScreen(true, {}) { enrolled += it; true } }
        compose.onNodeWithTag("pin-new").performTextInput("123456")
        compose.onNodeWithTag("pin-repeat").performTextInput("123457")
        compose.onNodeWithTag("pin-set").performClick()
        compose.onNodeWithTag("pin-error").assertIsDisplayed()
        // Both fields were cleared, so Set PIN waits for new input.
        compose.onNodeWithTag("pin-set").assertIsNotEnabled()
        compose.onNodeWithTag("pin-new").performTextInput("12ab3456")
        compose.onNodeWithTag("pin-repeat").performTextInput("123456")
        compose.onNodeWithTag("pin-set").performClick()
        compose.waitUntil { enrolled.isNotEmpty() }
        check(enrolled == listOf("123456")) { "Unexpected enrollment: $enrolled" }
    }

    @Test
    fun withoutScreenLockThePinIsExplainedAndSkipStays() {
        compose.setContent { SetupScreen(false, {}) { true } }
        compose.onNodeWithTag("lock-unavailable").assertIsDisplayed()
        compose.onNodeWithTag("pin-set").assertIsNotEnabled()
        compose.onNodeWithTag("lock-skip").assertIsEnabled()
    }
}
