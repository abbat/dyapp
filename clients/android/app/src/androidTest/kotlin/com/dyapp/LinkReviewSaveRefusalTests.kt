package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewField

/** Controlled native completion publishes typed state before releasing the admitted callback. */
class LinkReviewSaveRefusalTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun englishSaveRefusalsRetainChoicesAndOnlyPermittedRecovery() = verify("en")

    @Test
    fun russianSaveRefusalsRetainChoicesAndOnlyPermittedRecovery() = verify("ru")

    private fun verify(language: String) {
        val fixture = LinkReviewUiFixture(compose, completeLinkReviewFixture())
        val context = fixture.show(language, receivingFrame = true)
        for (scale in listOf(1f, 2f)) {
            for ((failure, label) in refusals()) {
                reset(fixture, scale)
                chooseAll()
                val before = fixture.saves.get()
                compose.onNodeWithTag("link-review-confirm").performScrollTo().performClick()
                compose.runOnIdle {
                    assertEquals(before + 1, fixture.saves.get())
                    val state = fixture.snapshot.value
                    fixture.snapshot.value = state.copy(enrollment = state.enrollment.copy(failure = failure))
                    fixture.confirmation.get().complete(Result.failure(failure))
                }
                compose.awaitNode(hasTestTag("link-setup-failure")).assertTextEquals(context.getString(label))
                compose.waitUntil(10_000) {
                    compose.onNodeWithTag("link-review-back").fetchSemanticsNode().config
                        .getOrNull(SemanticsProperties.Focused) == true
                }
                compose.awaitNode(SemanticsMatcher.expectValue(LinkReviewRecoveryReady, true))
                checkRecovery(failure)
                compose.onNodeWithTag("link-review-back").performClick()
                compose.awaitNode(hasTestTag("link-review-open"))
                compose.onNodeWithTag("link-setup-failure").assertTextEquals(context.getString(label))
            }
        }
    }

    private fun reset(fixture: LinkReviewUiFixture, scale: Float) {
        compose.runOnIdle {
            fixture.scale.floatValue = scale
            fixture.confirmation.set(CompletableDeferred())
            fixture.replace(completeLinkReviewFixture())
        }
    }

    private fun refusals() = listOf(
        LinkReviewException.Profile("invalid fields") to R.string.link_setup_selection,
        LinkReviewException.Selection("missing choice") to R.string.link_setup_selection,
        LinkReviewException.Failed("uncertain write") to R.string.link_review_uncertain,
        LinkReviewException.ReviewRequired() to R.string.link_setup_review,
        LinkReviewException.PrivateStateReviewRequired() to R.string.link_setup_private_review,
        LinkReviewException.EnrollmentAuthorizationRequired() to R.string.link_setup_authorization,
        LinkReviewException.Revoked() to R.string.link_setup_revoked,
        LinkReviewException.DeviceNotListed() to R.string.link_setup_unlisted
    )

    private fun chooseAll() {
        compose.awaitNode(hasTestTag("link-review-NAME-REMOTE") and isEnabled())
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").performScrollTo().performClick()
        }
    }

    private fun checkRecovery(failure: Throwable) {
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").assertIsSelected()
        }
        checkPermittedRecovery(failure)
        checkFeedbackOrder()
        for (tag in listOf("link-review-confirm", "link-review-back")) reachable(tag)
        if (failure is LinkReviewException.Failed || failure is LinkReviewException.ReviewRequired) {
            reachable("link-setup-prepare")
        }
    }

    private fun checkPermittedRecovery(failure: Throwable) {
        val retryable = failure is LinkReviewException.Profile || failure is LinkReviewException.Selection
        val confirm = compose.onNodeWithTag("link-review-confirm")
        if (retryable) confirm.assertIsEnabled() else confirm.assertIsNotEnabled()
        compose.onNodeWithTag("link-setup-finish").assertDoesNotExist()
        if (failure is LinkReviewException.Failed || failure is LinkReviewException.ReviewRequired) {
            compose.onNodeWithTag("link-setup-prepare").assertIsEnabled()
        } else {
            compose.onNodeWithTag("link-setup-prepare").assertDoesNotExist()
        }
    }

    private fun checkFeedbackOrder() {
        val error = compose.onNodeWithTag("link-setup-failure").getUnclippedBoundsInRoot()
        val save = compose.onNodeWithTag("link-review-confirm").getUnclippedBoundsInRoot()
        val last = compose.onNodeWithTag("link-review-${LinkReviewField.entries.last().name}-REMOTE")
            .getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("settings-receive").getUnclippedBoundsInRoot()
        assertTrue("Typed refusal follows all comparison groups", error.top >= last.bottom)
        assertTrue("Typed refusal precedes its recovery controls", error.bottom <= save.top)
        assertTrue(
            "Typed refusal bounds=$error viewport=$viewport save=$save",
            error.top >= viewport.top && error.bottom <= viewport.bottom
        )
    }

    private fun reachable(tag: String) {
        val bounds = compose.onNodeWithTag(tag).performScrollTo().assertIsDisplayed().getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("settings-receive").getUnclippedBoundsInRoot()
        assertTrue(
            "Recovery $tag bounds=$bounds viewport=$viewport",
            bounds.left >= viewport.left && bounds.right <= viewport.right &&
                bounds.top >= viewport.top && bounds.bottom <= viewport.bottom
        )
    }
}
