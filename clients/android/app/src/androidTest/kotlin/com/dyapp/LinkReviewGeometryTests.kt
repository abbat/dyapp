package com.dyapp

import android.content.Context
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewSource

class LinkReviewGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishComparisonKeepsAllOfferedValuesAtBothTextScales() = verify("en")

    @Test
    fun russianComparisonKeepsAllOfferedValuesAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = LinkReviewUiFixture(compose, completeLinkReviewFixture())
        val context = fixture.show(language)
        assertEquals(LinkReviewField.entries.toSet(), linkReviewLabels.keys)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle { fixture.scale.floatValue = scale }
            for (field in LinkReviewField.entries) {
                for (source in LinkReviewSource.entries) {
                    compose.onNodeWithTag("link-review-${field.name}-${source.name}").assertIsNotSelected()
                    readable(compose.onNodeWithTag("link-review-${field.name}-${source.name}-value"))
                }
            }
            exactForwardValues(context)
            coupledValues(context)
            compose.onNodeWithTag("link-review-confirm").assertIsNotEnabled()
        }
        photoReferencesDoNotSelectASource()
    }

    private fun exactForwardValues(context: Context) {
        compose.onNodeWithTag("link-review-INCOME-LOCAL-value")
            .assertTextContains((ULong.MAX_VALUE - 1uL).toString(), substring = true)
            .assertTextContains(ULong.MAX_VALUE.toString(), substring = true)
        compose.onNodeWithTag("link-review-GENDER-LOCAL-value")
            .assertTextContains(context.getString(R.string.link_review_unknown_value, UInt.MAX_VALUE.toString()))
        compose.onNodeWithTag("link-review-EDUCATION-LOCAL-value")
            .assertTextContains(context.getString(R.string.link_review_unknown_value, UInt.MAX_VALUE.toString()))
        compose.onNodeWithTag("link-review-INTERESTS-LOCAL-value")
            .assertTextContains("music; tea, coffee; learning languages; music")
        compose.onNodeWithTag("link-review-COMPANY-LOCAL-value").assertTextContains("Long company name ".repeat(8))
    }

    private fun coupledValues(context: Context) {
        compose.onNodeWithTag("link-review-BIRTH-LOCAL-value")
            .assertTextContains("2001", substring = true)
            .assertTextContains(context.getString(R.string.link_review_unpublished), substring = true)
            .assertTextContains("31", substring = true)
        compose.onNodeWithTag("link-review-LOCATION-LOCAL-value")
            .assertTextContains(UInt.MAX_VALUE.toString(), substring = true)
        compose.onNodeWithTag("link-review-SEEKING-LOCAL-value")
            .assertTextContains(context.getString(R.string.link_review_yes), substring = true)
            .assertTextContains(UInt.MAX_VALUE.toString(), substring = true)
            .assertTextContains("18", substring = true)
            .assertTextContains("120", substring = true)
    }

    private fun photoReferencesDoNotSelectASource() {
        val toggle = compose.onNodeWithTag("link-review-photo-LOCAL-0-references")
        toggle.performScrollTo().performClick()
        readable(compose.onNodeWithTag("link-review-photo-LOCAL-0-thumbnail"))
        for (index in 0..1) readable(compose.onNodeWithTag("link-review-photo-LOCAL-0-original-$index"))
        compose.onNodeWithTag("link-review-PHOTOS-LOCAL").assertIsNotSelected()
    }

    private fun readable(node: SemanticsNodeInteraction) {
        node.performScrollTo().assertIsDisplayed()
        assertTrue("Full comparison text fits its actual layout", node.textLayout().textFitsLayout())
        val value = node.getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("review-viewport").getUnclippedBoundsInRoot()
        assertTrue(
            "Values stay inside current viewport width",
            value.left >= viewport.left && value.right <= viewport.right
        )
    }
}
