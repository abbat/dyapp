package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Rule
import org.junit.Test

class SettingsFrameOwnerTests {
    @get:Rule
    val compose = createComposeRule()

    private fun click(tag: String) {
        compose.awaitNode(hasTestTag(tag)).performScrollTo().performClick()
    }

    @Test
    fun staleCategoryCallbackCannotNavigateAnotherIdentity() {
        var owner by mutableStateOf("first")
        compose.setContent {
            val navigation = rememberSettingsNavigation()
            MaterialTheme {
                val route = navigation.forOwner(owner)
                SettingsFrame(route) { pane ->
                    if (pane == SettingsPane.ACCOUNT) SettingsLink(SettingsPane.IDENTITIES, route)
                    Text(pane.name)
                }
            }
        }
        click("open-settings-account")
        val stale = compose.onNodeWithTag("open-settings-identities-screen")
            .fetchSemanticsNode()
            .config[SemanticsActions.OnClick]
        compose.runOnIdle { owner = "second" }
        compose.awaitNode(hasTestTag("settings-root"))
        compose.runOnIdle { stale.action?.invoke() }
        compose.onNodeWithTag("settings-root").assertExists()
        compose.onNodeWithTag("settings-identities-screen").assertDoesNotExist()
        click("open-settings-account")
        click("open-settings-identities-screen")
        click("settings-back")
        compose.onNodeWithTag("settings-account").assertExists()
        click("settings-back")
        compose.onNodeWithTag("settings-root").assertExists()
    }
}
