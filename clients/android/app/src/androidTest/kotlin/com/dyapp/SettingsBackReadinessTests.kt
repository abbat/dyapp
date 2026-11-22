package com.dyapp

import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class SettingsBackReadinessTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun rootNavigationRevealsBackAfterScrollingACollectionToItsEnd() {
        val route = mutableStateOf(SettingsPane.IDENTITIES)
        compose.setContent {
            MaterialTheme {
                BoxWithConstraints(Modifier.fillMaxSize()) {
                    val viewportHeight = maxHeight
                    SettingsFrame(route) {
                        Spacer(Modifier.height(viewportHeight))
                        Text("End of collection", Modifier.testTag("settings-end"))
                    }
                }
            }
        }
        for (pane in listOf(SettingsPane.IDENTITIES, SettingsPane.BLOCKED, SettingsPane.PASSED, SettingsPane.VIEWS)) {
            compose.runOnIdle { route.value = pane }
            compose.onNodeWithTag("settings-end").performScrollTo()
            val back = compose.onNodeWithTag("settings-back").getUnclippedBoundsInRoot()
            val page = compose.onNodeWithTag(pane.tag).getUnclippedBoundsInRoot()
            assertTrue("Back starts above the visible viewport", back.bottom <= page.top)
            compose.settingsRoot()
            compose.onNodeWithTag("settings-root").assertExists()
            compose.onNodeWithTag("settings-back").assertDoesNotExist()
        }
    }
}
