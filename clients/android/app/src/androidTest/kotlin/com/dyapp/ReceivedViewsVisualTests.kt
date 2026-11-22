package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** Dedicated CI-only source-bound matrix; ordinary instrumentation excludes this capture phase. */
class ReceivedViewsVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesEveryReceivedViewReadOutcome() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run through the dedicated native visual phase", arguments.getString("viewsVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val expected = ReceivedViewsVisualState.entries.size * 2 * 2 * 2 * 2
        val output = NativeScreenVisualOutput(source, width, "received-views-visual", expected)
        val host = ReceivedViewsVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (dark in listOf(false, true)) captureFontCases(host, output, language, dark)
        }
        output.finish()
    }

    private fun captureFontCases(
        host: ReceivedViewsVisualHost,
        output: NativeScreenVisualOutput,
        language: String,
        dark: Boolean
    ) {
        for (font in listOf(1f, 2f)) {
            val context = ownerVisualContext(language, font)
            for (state in ReceivedViewsVisualState.entries) {
                capture(host, output, ReceivedViewsVisualScene(state, context, dark, font))
                progress.completed()
            }
        }
    }

    private fun capture(
        host: ReceivedViewsVisualHost,
        output: NativeScreenVisualOutput,
        scene: ReceivedViewsVisualScene
    ) {
        try {
            host.show(scene)
            if (scene.state == ReceivedViewsVisualState.STALE) {
                compose.awaitNode(hasTestTag("received-view-open-1"))
                compose.onNodeWithTag("collection-refresh").performScrollTo().performClick()
            }
            compose.awaitNode(hasTestTag(scene.state.ready))
            if (scene.state !in listOf(ReceivedViewsVisualState.LOADING, ReceivedViewsVisualState.ERROR)) {
                compose.awaitNode(hasTestTag("collection-count"))
            }
            val geometry = NativeVisualGeometry(compose, scene.metadata.rootTag)
            geometry.first()
            output.capture(compose, scene.metadata, "top")
            geometry.check()
            output.capture(compose, scene.metadata, "bottom")
        } finally {
            host.hide()
        }
    }
}
