package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdatePlatform

class UpdatesCardTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    @Test
    fun updatesStayDisabledUntilTheFeedIsPublished() {
        val root = InstrumentationRegistry.getInstrumentation().targetContext.noBackupFilesDir.absolutePath
        MobileClientUpdates(root, MobileUpdatePlatform.ANDROID).use { client ->
            val previous = client.readState().interval
            try {
                client.setInterval(MobileUpdateInterval.WEEK)
                compose.shownPeerId()
                compose.openSettingsPane("settings-about")
                for (tag in listOf("update-6h", "update-12h", "update-24h", "update-7d", "update-check")) {
                    compose.onNodeWithTag(tag).performScrollTo().assertIsNotEnabled()
                }
                compose.awaitNode(hasTestTag("update-7d") and isSelected())
                compose.onNodeWithTag("update-7d").assertIsSelected()
                compose.onNodeWithTag("update-state").performScrollTo()
                    .assertTextEquals(compose.activity.getString(R.string.update_not_published))
            } finally {
                client.setInterval(previous)
            }
        }
    }
}
