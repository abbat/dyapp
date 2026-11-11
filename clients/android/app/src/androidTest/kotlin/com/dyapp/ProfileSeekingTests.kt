package com.dyapp

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.profileScales

class ProfileSeekingTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun save(status: Int) {
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(status))
    }

    private fun row(name: Int, value: String) =
        compose.activity.getString(R.string.profile_preview_row, string(name), value)

    private fun typeAges(from: String, to: String) {
        compose.onNodeWithTag("profile-seeking-age-from").performScrollTo().performTextReplacement(from)
        compose.onNodeWithTag("profile-seeking-age-to").performScrollTo().performTextReplacement(to)
    }

    private fun openPublish() {
        compose.onNodeWithTag("profile-publish").performScrollTo().performClick()
    }

    private fun closePublish() {
        compose.onNodeWithTag("profile-cancel").performClick()
    }

    private fun previewShowsTheSavedChoices(woman: UInt) {
        openPublish()
        compose.onNodeWithTag("preview-seeking-genders").performScrollTo()
            .assertTextEquals(row(R.string.profile_seeking_genders, string(R.string.profile_value_gender_Woman)))
        compose.onNodeWithTag("preview-seeking-age-from").performScrollTo()
            .assertTextEquals(row(R.string.profile_seeking_age_from, "25"))
        compose.onNodeWithTag("preview-not-looking").assertDoesNotExist()
        closePublish()
        compose.onNodeWithTag("profile-seeking-gender-$woman").performScrollTo().assertIsOn()
    }

    private fun notLookingClearsTheChoices(woman: UInt) {
        compose.onNodeWithTag("profile-not-looking").performScrollTo().performClick()
        compose.onNodeWithTag("profile-seeking-gender-$woman").performScrollTo().assertIsNotEnabled()
        compose.onNodeWithTag("profile-seeking-age-from").performScrollTo().assertIsNotEnabled()
        save(R.string.profile_saved)
        compose.onNodeWithTag("profile-seeking-gender-$woman").performScrollTo().assertIsOff()
        compose.onNodeWithTag("profile-seeking-age-from").performScrollTo().assertTextContains("", substring = false)
        // Clearing on save is no edit, so publishing stays open.
        openPublish()
        compose.onNodeWithTag("preview-not-looking").performScrollTo()
            .assertTextEquals(row(R.string.profile_not_looking, string(R.string.profile_not_looking_on)))
        compose.onNodeWithTag("preview-seeking-genders").assertDoesNotExist()
        compose.onNodeWithTag("preview-seeking-age-from").assertDoesNotExist()
        closePublish()
    }

    @Test
    fun whoYouSeekMapsToTheRustRecordAndBack() {
        val draft = ProfileDraft(seeking = ProfileSeeking(listOf(1u, 2u), 18u, 40u))
        check(draft.record().draft() == draft) { "${draft.record()}" }
        val away = ProfileDraft(seeking = ProfileSeeking(notLooking = true))
        check(away.record().draft() == away) { "${away.record()}" }
    }

    @Test
    fun whoYouSeekIsCheckedShownAndClearedByTheSwitch() {
        val woman = profileScales().first { it.key == GENDER }.choices.first { it.key == "Woman" }.code
        compose.skipNetworkEntry()
        compose.openProfileEditor()
        typeAges("40", "30")
        save(R.string.profile_seeking_invalid)
        typeAges("x", "")
        save(R.string.profile_seeking_invalid)
        typeAges("25", "")
        compose.onNodeWithTag("profile-seeking-gender-$woman").performScrollTo().performClick()
        save(R.string.profile_saved)
        previewShowsTheSavedChoices(woman)
        notLookingClearsTheChoices(woman)
        // Later tests start with the switch off.
        compose.onNodeWithTag("profile-not-looking").performScrollTo().performClick()
        save(R.string.profile_saved)
    }
}
