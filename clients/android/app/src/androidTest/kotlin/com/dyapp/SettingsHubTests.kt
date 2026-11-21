package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class SettingsHubTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun click(tag: String) {
        compose.awaitNode(hasTestTag(tag) and isEnabled()).performScrollTo().performClick()
    }

    @Test
    fun categoriesHideCollectionsAndRestoreShellOnBack() {
        compose.shownPeerId()
        compose.openSettingsPane()
        for (tag in listOf("settings-create", "settings-remove", "settings-identities", "unblock-1", "unpass-1")) {
            compose.onNodeWithTag(tag).assertDoesNotExist()
        }
        click("open-settings-photos")
        compose.onNodeWithTag("app-ready").assertDoesNotExist()
        compose.onNodeWithTag("nav-profile").assertDoesNotExist()
        compose.awaitNode(hasTestTag("photo-mode-network") and isEnabled())
        compose.onNodeWithTag("settings-back").performClick()
        compose.awaitNode(hasTestTag("settings-root"))
        compose.onNodeWithTag("nav-profile").assertExists()
        click("open-settings-account")
        click("open-settings-identities-screen")
        compose.identityRows()
        compose.onNodeWithTag("settings-back").performClick()
        compose.awaitNode(hasTestTag("settings-account"))
        compose.onNodeWithTag("settings-create").assertDoesNotExist()
        compose.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        compose.awaitNode(hasTestTag("settings-root"))
    }
}
