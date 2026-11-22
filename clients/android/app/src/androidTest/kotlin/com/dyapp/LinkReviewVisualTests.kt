package com.dyapp

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName
import uniffi.dyapp.LinkReviewException
import uniffi.dyapp.LinkReviewField
import java.io.IOException

internal enum class LinkReviewVisualState { CHOICES, PREVIEW, SAVING, REFUSED, PHOTO_UNAVAILABLE }

/** Dedicated full-form matrix; ordinary runtime excludes it to retain its existing exposure budget. */
class LinkReviewVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesEnglishCompleteComparison() = captureLanguage("en")

    @Test
    fun capturesRussianCompleteComparison() = captureLanguage("ru")

    private fun captureLanguage(language: String) {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Dedicated comparison visual phase", arguments.getString("comparisonVisual") == "1")
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val source = checkNotNull(arguments.getString("sourceSha"))
        val fixture = LinkReviewUiFixture(compose, completeLinkReviewFixture())
        val context = fixture.show(language, receivingFrame = true)
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val output = LinkReviewVisualOutput(compose, source, width, language)
        for (dark in listOf(false, true)) {
            for (font in listOf(1f, 2f)) {
                captureScale(fixture, output, dark, font)
            }
        }
        output.finish()
    }

    private fun captureScale(fixture: LinkReviewUiFixture, output: LinkReviewVisualOutput, dark: Boolean, font: Float) {
        for (state in LinkReviewVisualState.entries) {
            prepare(fixture, state, dark, font)
            output.start(state, tags(state), font, dark)
            val geometry = NativeVisualGeometry(compose, "settings-receive", output::capture)
            geometry.first()
            output.capture()
            geometry.check()
            output.capture()
            output.finishScene()
            progress.completed()
        }
    }

    private fun reset(fixture: LinkReviewUiFixture, state: LinkReviewVisualState, dark: Boolean, font: Float) {
        compose.runOnIdle {
            fixture.darkTheme = dark
            fixture.scale.floatValue = font
            fixture.confirmation.set(CompletableDeferred())
            fixture.photo.set(
                CompletableDeferred(
                    if (state == LinkReviewVisualState.PHOTO_UNAVAILABLE) {
                        Result.failure(IOException("unavailable"))
                    } else {
                        Result.success(fixture.pixels)
                    }
                )
            )
            fixture.replace(completeLinkReviewFixture())
            fixture.expectedPreview = completeLinkReviewFixture().remote
        }
        compose.awaitNode(hasTestTag("link-review-NAME-REMOTE") and isEnabled())
    }

    private fun prepare(fixture: LinkReviewUiFixture, state: LinkReviewVisualState, dark: Boolean, font: Float) {
        reset(fixture, state, dark, font)
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").assertIsNotSelected()
        }
        if (state in listOf(LinkReviewVisualState.CHOICES, LinkReviewVisualState.PHOTO_UNAVAILABLE)) return
        for (field in LinkReviewField.entries) {
            compose.onNodeWithTag("link-review-${field.name}-REMOTE").performScrollTo().performClick()
        }
        if (state == LinkReviewVisualState.PREVIEW) {
            compose.onNodeWithTag("link-review-preview").performScrollTo().performClick()
            compose.awaitNode(hasTestTag("link-review-edit"))
        } else {
            submit(fixture, state)
        }
    }

    private fun submit(fixture: LinkReviewUiFixture, state: LinkReviewVisualState) {
        compose.onNodeWithTag("link-review-confirm").performScrollTo().performClick()
        compose.runOnIdle {
            val before = fixture.snapshot.value
            val failure = if (state == LinkReviewVisualState.REFUSED) LinkReviewException.Profile("invalid") else null
            fixture.snapshot.value = before.copy(
                busy = failure == null,
                enrollment = before.enrollment.copy(
                    failure = failure,
                    operation = if (failure == null) LinkEnrollmentOperation.CONFIRM else null
                )
            )
            if (failure != null) fixture.confirmation.get().complete(Result.failure(failure))
        }
        if (state == LinkReviewVisualState.REFUSED) {
            compose.waitUntil(10_000) {
                compose.onNodeWithTag("link-review-back").fetchSemanticsNode().config
                    .getOrNull(SemanticsProperties.Focused) == true
            }
        }
    }

    private fun tags(state: LinkReviewVisualState): Set<String> = LinkReviewField.entries.flatMap { field ->
        val sources = if (state == LinkReviewVisualState.PREVIEW) listOf("REMOTE") else listOf("LOCAL", "REMOTE")
        sources.map { "link-review-${field.name}-$it-value" }
    }.toSet()
}
