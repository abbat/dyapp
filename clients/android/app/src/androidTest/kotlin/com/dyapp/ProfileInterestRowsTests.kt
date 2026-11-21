package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import org.junit.Rule
import org.junit.Test

/** The actual rows are shared by fetched profiles and the saved publication preview. */
class ProfileInterestRowsTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun tagsKeepPunctuationAndOrderAndAnInterestOnlyProfileIsNotEmpty() {
        val draft = mutableStateOf(ProfileDraft(interests = listOf("music", "tea, coffee", "music")))
        compose.setContent {
            DYAppTheme { Column { ProfileRows(draft.value, emptyList()) } }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val expected = context.getString(
            R.string.profile_preview_row,
            context.getString(R.string.profile_interests),
            "music; tea, coffee; music"
        )
        compose.onNodeWithTag("preview-interests").assertTextEquals(expected)
        compose.onNodeWithTag("preview-about").assertDoesNotExist()
        compose.onNodeWithText(context.getString(R.string.profile_preview_empty)).assertDoesNotExist()
        compose.runOnIdle { draft.value = ProfileDraft(interests = emptyList()) }
        compose.onNodeWithTag("preview-interests").assertDoesNotExist()
        compose.onNodeWithText(context.getString(R.string.profile_preview_empty)).assertExists()
    }
}
