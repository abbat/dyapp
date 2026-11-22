package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewSource

class LinkReviewFormTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun previewPreservesChoicesAndEqualReplacementRejectsActualRetiredCallbacks() {
        val fixture = LinkReviewUiFixture(compose)
        fixture.show()
        val oldChoose = action("link-review-NAME-LOCAL")
        compose.onNodeWithTag("link-review-NAME-LOCAL").performScrollTo().performClick()
        val oldSave = action("link-review-confirm")
        val oldPreview = action("link-review-preview")
        compose.onNodeWithTag("link-review-preview").performScrollTo().performClick()
        assertEquals(1, fixture.previews.get())
        focusReady("link-review-edit")
        compose.onNodeWithTag("link-review-edit").performScrollTo().performClick()
        focusReady("link-review-preview")
        compose.onNodeWithTag("link-review-NAME-LOCAL").assertIsSelected()
        val prior = checkNotNull(fixture.snapshot.value.enrollment.comparison).snapshot
        compose.runOnIdle { fixture.replace(prior.copy()) }
        compose.onNodeWithTag("link-review-NAME-LOCAL").assertIsNotSelected()
        compose.runOnIdle {
            oldChoose()
            oldSave()
            oldPreview()
        }
        compose.onNodeWithTag("link-review-NAME-LOCAL").assertIsNotSelected()
        assertEquals(1, fixture.previews.get())
        assertEquals(0, fixture.saves.get())
    }

    @Test
    fun confirmationUsesActualCompletionEvenWhenNoBusyFrameIsRendered() {
        val fixture = LinkReviewUiFixture(compose)
        fixture.show()
        compose.onNodeWithTag("link-review-confirm").assertIsNotEnabled()
        compose.onNodeWithTag("link-review-NAME-LOCAL").performScrollTo().performClick()
        val save = action("link-review-confirm")
        compose.runOnIdle {
            save()
            save()
        }
        assertEquals(1, fixture.saves.get())
        assertEquals(LinkReviewSource.LOCAL, fixture.choices.get().single().source)
        compose.onNodeWithTag("link-review-confirm").assertIsNotEnabled()
        compose.runOnIdle {
            fixture.confirmation.get().complete(
                Result.failure(LinkReviewException.Profile("invalid"))
            )
        }
        compose.waitUntil(5_000) {
            !compose.onNodeWithTag("link-review-confirm").fetchSemanticsNode().config
                .contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled)
        }
        compose.onNodeWithTag("link-review-confirm").assertIsEnabled()
        compose.runOnIdle { save() }
        assertEquals(2, fixture.saves.get())
    }

    private fun focusReady(tag: String) {
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag(tag).fetchSemanticsNodes().singleOrNull()
                ?.config
                ?.getOrNull(SemanticsProperties.Focused) == true
        }
        compose.onNodeWithTag(tag).assertIsFocused()
    }

    private fun action(tag: String): () -> Unit =
        compose.onNodeWithTag(tag).fetchSemanticsNode().config[applicationClickKey]
}
