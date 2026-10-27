package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import java.time.LocalDate
import java.time.ZoneOffset
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

class ProfileDraftTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    @Test
    fun previewShowsOnlyTheSavedDraftWithAgeAndWarnings() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("profile-edit").performScrollTo().performClick()
        val parts = listOf("profile-birth-year" to "2000", "profile-birth-month" to "1", "profile-birth-day" to "1")
        for ((tag, value) in parts) {
            compose.onNodeWithTag(tag).performScrollTo().performTextClearance()
            compose.onNodeWithTag(tag).performTextInput(value)
        }
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        // Born on 1 January, so the age is exact on any day of the year.
        val age = LocalDate.now(ZoneOffset.UTC).year - 2000
        compose.onNodeWithTag("profile-preview-age").performScrollTo()
            .assertTextEquals(compose.activity.getString(R.string.profile_age, "$age"))
        compose.onNodeWithTag("profile-public-disclosure").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("profile-full-date").performScrollTo().assertIsDisplayed()

        // An unsaved edit does not change the preview; a saved one does.
        compose.onNodeWithTag("profile-birth-year").performScrollTo().performTextClearance()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        compose.onNodeWithTag("profile-full-date").assertExists()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        val unknown = compose.activity.getString(R.string.profile_age, string(R.string.profile_age_unknown))
        compose.onNodeWithTag("profile-preview-age").performScrollTo().assertTextEquals(unknown)
        compose.onNodeWithTag("profile-full-date").assertDoesNotExist()
    }

    @Test
    fun placePickerSavesOnlyASuggestedPlace() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("profile-edit").performScrollTo().performClick()
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
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        val lang = compose.activity.resources.configuration.locales[0].language
        val berlin = compose.activity.getString(R.string.profile_preview_row, string(R.string.profile_place),
            placeText("DE", berlinId, lang))
        compose.onNodeWithTag("preview-place").performScrollTo().assertTextEquals(berlin)
        compose.onNodeWithTag("preview-country").assertExists()

        // Clearing the country clears the place.
        compose.onNodeWithTag("profile-country").performScrollTo().performClick()
        compose.onNodeWithTag("country-none").performClick()
        compose.onNodeWithTag("profile-save").performScrollTo().performClick()
        compose.onNodeWithTag("profile-status").assertTextEquals(string(R.string.profile_saved))
        compose.onNodeWithTag("profile-preview").performScrollTo().performClick()
        compose.onNodeWithTag("preview-place").assertDoesNotExist()
        compose.onNodeWithTag("preview-country").assertDoesNotExist()
    }
}
