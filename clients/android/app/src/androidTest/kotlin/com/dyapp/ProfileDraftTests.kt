package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.profileScales
import java.time.LocalDate
import java.time.ZoneOffset

class ProfileDraftTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun openPublish() {
        compose.onNodeWithTag("profile-publish").performScrollTo().performClick()
    }

    private fun closePublish() {
        compose.onNodeWithTag("profile-cancel").performClick()
    }

    @Test
    fun previewShowsOnlyTheSavedDraftWithAgeAndWarnings() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("birth-year").performTextInput("2000")
        pickBirthPart("birth-month")
        pickBirthPart("birth-day")
        // Born on 1 January, so the age is exact on any day of the year.
        val age = LocalDate.now(ZoneOffset.UTC).year - 2000
        compose.onNodeWithTag("profile-age").performScrollTo()
            .assertTextEquals(compose.activity.getString(R.string.profile_age_search, "$age"))
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        openPublish()
        compose.onNodeWithTag("profile-preview-age").performScrollTo()
            .assertTextEquals(compose.activity.getString(R.string.profile_age, "$age"))
        compose.onNodeWithTag("profile-public-disclosure").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("profile-full-date").performScrollTo().assertIsDisplayed()
        closePublish()

        // An unsaved edit blocks the dialog; a saved one changes what it shows.
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-age").assertDoesNotExist()
        compose.onNodeWithTag("profile-publish").performScrollTo().assertIsNotEnabled()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        openPublish()
        val unknown = compose.activity.getString(R.string.profile_age, string(R.string.profile_age_unknown))
        compose.onNodeWithTag("profile-preview-age").performScrollTo().assertTextEquals(unknown)
        compose.onNodeWithTag("profile-full-date").assertDoesNotExist()
        closePublish()
    }

    private fun pickBirthPart(tag: String) {
        compose.onNodeWithTag(tag).performScrollTo().performClick()
        compose.onNodeWithTag("$tag-1").performScrollTo().performClick()
        compose.onNodeWithTag(tag).assertTextEquals("1")
    }

    @Test
    fun everyScaleAndAboutMapToTheRustRecordAndBack() {
        val draft = ProfileDraft(profileScales().associate { it.key to it.choices.first().code.toInt() }, about = "Hi")
        check(draft.record().draft() == draft) { "${draft.record()}" }
    }

    @Test
    fun otherScalesStayFoldedUntilProfileMoreOpens() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        val tag = "profile-" + profileScales().first { it.key !in BASIC_SCALES }.key
        compose.onNodeWithTag(tag).assertDoesNotExist()
        compose.onNodeWithTag("profile-more").performScrollTo().performClick()
        compose.onNodeWithTag(tag).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun multiWordScaleSurvivesReopeningTheEditor() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        // Its Rust key has an underscore, which the client.db record maps by name.
        val scale = profileScales().first { it.key == "relationship_goal" }
        val choice = label("value.${scale.key}.${scale.choices.first().key}")
        compose.onNodeWithTag("profile-goal").performScrollTo().performClick()
        // A retried run can already show the choice on the field itself.
        compose.onNode(hasText(choice) and !hasTestTag("profile-goal")).performClick()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(R.string.profile_saved))
        compose.onNodeWithTag("nav-search").performClick()
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("profile-goal").performScrollTo().assertTextEquals(choice)
    }

    private fun label(key: String) = compose.activity.run {
        getString(resources.getIdentifier("profile_" + key.replace('.', '_'), "string", packageName))
    }

    @Test
    fun placePickerSavesOnlyASuggestedPlace() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("profile-country").performScrollTo().performClick()
        compose.onNodeWithTag("country-DE").performScrollTo().performClick()
        compose.onNodeWithTag("profile-place").performScrollTo().performTextInput("Nowhere")
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(R.string.profile_place_unknown))

        // "Berl" suggests Berlin first; picking it saves its GeoNames ID.
        val berlinId = 2_950_159
        compose.onNodeWithTag("profile-place").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-place").performTextInput("Berl")
        compose.onNodeWithTag("place-$berlinId").performScrollTo().performClick()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(R.string.profile_saved))
        openPublish()
        val lang = compose.activity.resources.configuration.locales[0].language
        val berlin = compose.activity.getString(
            R.string.profile_preview_row,
            string(R.string.profile_place),
            placeText("DE", berlinId, lang)
        )
        compose.onNodeWithTag("preview-place").performScrollTo().assertTextEquals(berlin)
        compose.onNodeWithTag("preview-country").assertExists()
        closePublish()
        clearCountryClearsPlace()
    }

    private fun clearCountryClearsPlace() {
        compose.onNodeWithTag("profile-country").performScrollTo().performClick()
        compose.onNodeWithTag("country-none").performScrollTo().performClick()
        compose.onNodeWithTag("profile-country").assertTextEquals(string(R.string.profile_unset))
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(R.string.profile_saved))
        openPublish()
        compose.onNodeWithTag("preview-place").assertDoesNotExist()
        compose.onNodeWithTag("preview-country").assertDoesNotExist()
        closePublish()
    }
}
