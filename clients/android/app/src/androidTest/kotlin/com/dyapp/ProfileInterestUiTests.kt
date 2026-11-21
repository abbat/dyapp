package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.profileInterestLimits

class ProfileInterestUiTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun replace(typed: String) {
        compose.onNodeWithTag("profile-interests").performScrollTo().performTextReplacement(typed)
    }

    private fun assertInput(expected: String) {
        val actual = compose.onNodeWithTag("profile-interests").performScrollTo()
            .fetchSemanticsNode()
            .config[SemanticsProperties.EditableText].text
        assertEquals(expected, actual)
    }

    private fun reopen() {
        compose.onNodeWithTag("nav-search").performClick()
        compose.openProfileEditor()
    }

    @Test
    fun editedTagsReopenCanonicalAndSavingBlankClearsThem() {
        compose.skipNetworkEntry()
        compose.openProfileEditor()
        replace(" MUSIC \nTea, Coffee\nmusic")
        compose.saveProfile()
        reopen()
        assertInput("music\ntea, coffee\nmusic")
        replace("")
        compose.saveProfile()
        reopen()
        assertInput("")
    }

    @Test
    fun overlongTagReportsValidationAndKeepsInputEditable() {
        compose.skipNetworkEntry()
        compose.openProfileEditor()
        val invalid = "x".repeat(profileInterestLimits().maxTagChars.toInt() + 1)
        replace(invalid)
        compose.saveProfile()
        compose.onNodeWithTag("profile-status").assertTextEquals(
            compose.activity.getString(R.string.profile_interests_invalid)
        )
        assertInput(invalid)
        replace("recovered")
        compose.saveProfile()
        compose.onNodeWithTag("profile-status").assertTextEquals(compose.activity.getString(R.string.profile_saved))
    }
}
