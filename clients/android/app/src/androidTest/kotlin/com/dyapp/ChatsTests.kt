package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class ChatsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    @Test
    fun chatsTabOfANewIdentityIsEmptyAndHasNoBadge() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-chats").performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.nav_chats))
        compose.onNodeWithTag("chats").assertExists()
        compose.onNodeWithText(string(R.string.chats_empty)).assertExists()
        compose.onNodeWithText(string(R.string.chats_identity)).assertExists()
        compose.onNodeWithTag("chats-unread", useUnmergedTree = true).assertDoesNotExist()
        // The first receive fails without a network; the app stays usable and the section still opens.
        compose.openProfileEditor()
        compose.onNodeWithTag("peer-id").assertExists()
    }
}
