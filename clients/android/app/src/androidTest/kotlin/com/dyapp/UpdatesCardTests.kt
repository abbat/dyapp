package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class UpdatesCardTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    @Test
    fun updatesStayDisabledUntilTheFeedIsPublished() {
        compose.shownPeerId()
        compose.onNodeWithTag("nav-settings").performClick()
        for (tag in listOf("update-6h", "update-12h", "update-24h", "update-7d", "update-check")) {
            compose.onNodeWithTag(tag).performScrollTo().assertIsNotEnabled()
        }
        compose.onNodeWithTag("update-24h").assertIsSelected()
        compose.onNodeWithTag("update-state").performScrollTo()
            .assertTextEquals(compose.activity.getString(R.string.update_not_published))
    }
}
