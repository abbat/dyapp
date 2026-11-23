package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LinkReviewField
import java.io.IOException

/** A full comparison exposes refusal feedback that scrolling to its controls could otherwise hide. */
class LinkReviewFailureTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishRejectedPreviewStaysBesideSafeRecoveryAtBothTextScales() = verify("en")

    @Test
    fun russianRejectedPreviewStaysBesideSafeRecoveryAtBothTextScales() = verify("ru")

    private fun verify(language: String) {
        val fixture = LinkReviewUiFixture(compose, completeLinkReviewFixture())
        fixture.show(language, receivingFrame = true)
        for (scale in listOf(1f, 2f)) {
            compose.runOnIdle {
                fixture.scale.floatValue = scale
                fixture.replace(completeLinkReviewFixture())
                fixture.previewFailure = IOException("Controlled read-only preview refusal at $scale")
            }
            chooseAll()
            val before = fixture.previews.get()
            compose.onNodeWithTag("link-review-preview").performScrollTo().performClick()
            compose.awaitNode(hasTestTag("link-review-preview-error"))
            awaitSafeFocus()
            checkRecovery(fixture, before + 1, scale)
        }
    }

    private fun checkRecovery(fixture: LinkReviewUiFixture, previews: Int, scale: Float) {
        assertEquals(previews, fixture.previews.get())
        assertEquals(0, fixture.saves.get())
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").assertIsSelected()
        }
        visible("link-review-preview-error")
        assertEquals(
            scale,
            compose.onNodeWithTag("link-review-preview-error").textLayout().layoutInput.density.fontScale
        )
        for (tag in listOf("link-review-preview", "link-review-confirm", "link-review-back")) {
            compose.onNodeWithTag(tag).assertIsEnabled().performScrollTo()
            visible(tag)
        }
    }

    private fun chooseAll() {
        compose.awaitNode(hasTestTag("link-review-NAME-REMOTE") and isEnabled())
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").performScrollTo().assertIsEnabled().performClick()
        }
    }

    private fun awaitSafeFocus() {
        compose.waitUntil(10_000) {
            compose.onNodeWithTag("link-review-back").fetchSemanticsNode().config
                .getOrNull(SemanticsProperties.Focused) == true
        }
        compose.onNodeWithTag("link-review-back").assertIsFocused()
        compose.awaitNode(SemanticsMatcher.expectValue(LinkReviewRecoveryReady, true))
    }

    private fun visible(tag: String) {
        val bounds = compose.onNodeWithTag(tag).assertIsDisplayed().getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("settings-receive").getUnclippedBoundsInRoot()
        assertTrue(
            "Refusal/recovery $tag bounds=$bounds viewport=$viewport",
            bounds.left >= viewport.left && bounds.right <= viewport.right &&
                bounds.top >= viewport.top && bounds.bottom <= viewport.bottom
        )
    }
}
