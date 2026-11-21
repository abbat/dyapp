package com.dyapp

import android.content.Context
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class InterestGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishInterestFieldsAndPublicRowsFitNormalAndLargeText() = verify("en")

    @Test
    fun russianInterestFieldsAndPublicRowsFitNormalAndLargeText() = verify("ru")

    private fun verify(language: String) {
        val fixture = InterestGeometryFixture(compose)
        val context = fixture.show(language)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.value = scale }
            requireFields(fixture, context)
            val expected = context.getString(
                R.string.profile_preview_row,
                context.getString(R.string.profile_interests),
                fixture.tags.joinToString("; ")
            )
            val public = compose.onNodeWithTag("preview-interests").assertTextEquals(expected)
            readable(public)
            val editor = compose.onNodeWithTag("profile-interests")
            editor.performScrollTo()
            val layout = editor.textLayout()
            assertEquals(fixture.tags.joinToString("\n"), layout.layoutInput.text.text)
            assertTrue("All multiline input must fit its current layout", layout.textFitsLayout())
        }
    }

    private fun requireFields(fixture: InterestGeometryFixture, context: Context) {
        for (resource in listOf(R.string.profile_interests, R.string.search_interest)) {
            readable(compose.onNodeWithText(context.getString(resource), useUnmergedTree = true))
        }
        val profileHint = context.getString(
            R.string.profile_interests_hint,
            fixture.limits.maxTags.toString(),
            fixture.limits.maxTagChars.toString()
        )
        readable(compose.onNodeWithText(profileHint, useUnmergedTree = true))
        val searchHint = context.getString(R.string.search_interest_hint, fixture.limits.maxTagChars.toString())
        readable(compose.onNodeWithText(searchHint, useUnmergedTree = true))
    }

    private fun readable(node: SemanticsNodeInteraction) {
        node.performScrollTo().assertIsDisplayed()
        assertTrue("The complete interest copy must fit its text layout", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("interest-geometry").getUnclippedBoundsInRoot()
        assertTrue("Interest copy stays inside current viewport width", bounds.left >= viewport.left)
        assertTrue("Interest copy stays inside current viewport width", bounds.right <= viewport.right)
    }
}
