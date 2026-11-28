package com.dyapp

import android.graphics.RectF
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.text.TextRange
import androidx.test.espresso.Espresso.closeSoftKeyboard
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** Real keyboard/editor phase; controlled preparation still authorizes no native transfer. */
class OwnerApprovalImeTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun codeSelectionAndActionsRemainVisibleWithTheRealKeyboard() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run only through the real-IME phase", arguments.getString("ownerIme") == "1")
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val source = checkNotNull(arguments.getString("sourceSha"))
        val evidence = OwnerImeEvidence(source, width)
        val host = OwnerVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (font in listOf(1f, 2f)) {
                for (dark in listOf(false, true)) {
                    val scene = OwnerVisualScene(
                        OwnerVisualBackend(OwnerVisualState.INPUT),
                        ownerVisualContext(language, font),
                        dark,
                        font
                    )
                    exercise(host, evidence, scene)
                    progress.completed()
                }
            }
        }
        evidence.finish()
    }

    private fun exercise(host: OwnerVisualHost, evidence: OwnerImeEvidence, scene: OwnerVisualScene) {
        val backend = scene.backend
        try {
            host.show(scene)
            OwnerVisualDriver(compose).show(backend)
            val input = compose.onNodeWithTag("owner-code-input").performScrollTo().performClick()
            compose.awaitNode(hasTestTag("owner-code-input") and isFocused())
            val window = OwnerImeWindow(compose)
            window.awaitVisible()
            input.performTextInput(OWNER_VISUAL_CODE)
            inspectEditor(host, evidence, scene, window)
            compose.onNodeWithTag("owner-code-next").performClick()
            compose.awaitNode(hasTestTag("owner-recipient")).assertTextEquals(OWNER_VISUAL_CODE)
        } finally {
            try {
                closeSoftKeyboard()
            } finally {
                try {
                    host.hide(scene)
                } finally {
                    backend.close()
                }
            }
        }
    }

    private fun inspectEditor(
        host: OwnerVisualHost,
        evidence: OwnerImeEvidence,
        scene: OwnerVisualScene,
        window: OwnerImeWindow
    ) {
        val glyphs = OwnerImeCharacters(compose, window)
        for (index in listOf(0, OWNER_VISUAL_CODE.length / 2, OWNER_VISUAL_CODE.length)) {
            glyphs.select(index)
            awaitSelection(index)
            window.awaitVisible()
            val glyph = try {
                glyphs.requireVisible(index.coerceAtMost(OWNER_VISUAL_CODE.lastIndex))
            } catch (failure: AssertionError) {
                runCatching { evidence.captureFailure(host, index) }.exceptionOrNull()?.let(failure::addSuppressed)
                throw failure
            }
            val measured = OwnerImeMeasurement(
                RectF(window.visibleFrame()),
                glyph,
                window.requireControl("owner-cancel"),
                window.requireControl("owner-code-next")
            )
            evidence.capture(host, scene, index, measured)
        }
    }

    private fun awaitSelection(index: Int) {
        compose.waitUntil(10_000) {
            val config = compose.onNodeWithTag("owner-code-input").fetchSemanticsNode().config
            config[SemanticsProperties.EditableText].text == OWNER_VISUAL_CODE &&
                config[SemanticsProperties.TextSelectionRange] == TextRange(index)
        }
    }
}
