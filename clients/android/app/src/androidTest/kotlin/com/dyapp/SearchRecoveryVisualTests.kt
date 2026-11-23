package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** Controlled native presentation evidence, separate from actual JNI authorization tests. */
class SearchRecoveryVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesOriginalOwnerReviewAndDeliveryOutcomesAtActualTextScales() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run through the dedicated native visual phase", arguments.getString("recoveryVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val output = NativeScreenVisualOutput(
            source,
            width,
            "search-recovery-visual",
            SearchRecoveryVisualState.entries.size * 2 * 2 * 2 * 2
        )
        val host = SearchRecoveryVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (dark in listOf(false, true)) captureFonts(host, output, language, dark)
        }
        output.finish()
    }

    private fun captureFonts(
        host: SearchRecoveryVisualHost,
        output: NativeScreenVisualOutput,
        language: String,
        dark: Boolean
    ) {
        for (font in listOf(1f, 2f)) {
            for (state in SearchRecoveryVisualState.entries) {
                capture(host, output, SearchRecoveryVisualScene(state, ownerVisualContext(language, font), dark, font))
                progress.completed()
            }
        }
    }

    private fun capture(
        host: SearchRecoveryVisualHost,
        output: NativeScreenVisualOutput,
        scene: SearchRecoveryVisualScene
    ) {
        try {
            host.show(scene)
            ready(scene)
            val geometry = NativeVisualGeometry(compose, scene.metadata.rootTag)
            geometry.first()
            output.capture(compose, scene.metadata, "top")
            geometry.check()
            output.capture(compose, scene.metadata, "bottom")
        } finally {
            try { host.hide() } finally { scene.close() }
        }
    }

    private fun ready(scene: SearchRecoveryVisualScene) {
        compose.awaitSnapshot(scene.session.state, 5_000, "search recovery prepared") {
            !it.busy && (it.opening != null || it.failure != null)
        }
        if (scene.state != SearchRecoveryVisualState.REVIEW && scene.state.preparationFailure == null) {
            val opening = checkNotNull(scene.session.state.value.opening)
            checkNotNull(scene.session.confirm(opening))
            if (scene.state == SearchRecoveryVisualState.BUSY) {
                compose.waitUntil(5_000) { scene.entered.isCompleted }
            } else {
                compose.awaitSnapshot(scene.session.state, 5_000, "search recovery confirmed") { !it.busy }
                if (scene.state == SearchRecoveryVisualState.RETRY_ERROR) {
                    checkNotNull(scene.session.retry(opening))
                    compose.awaitSnapshot(scene.session.state, 5_000, "search recovery retry") { !it.busy }
                }
            }
        }
        checkPresentation(scene)
    }

    private fun checkPresentation(scene: SearchRecoveryVisualScene) {
        val busy = scene.state == SearchRecoveryVisualState.BUSY
        val sent = scene.state == SearchRecoveryVisualState.SENT
        val close = compose.awaitNode(hasTestTag("search-recovery-close"))
        if (busy) close.assertIsNotEnabled() else close.assertIsEnabled()
        val action = compose.onNodeWithTag("search-recovery-action")
        if (sent) action.assertDoesNotExist() else if (busy) action.assertIsNotEnabled() else action.assertIsEnabled()
        compose.onNodeWithTag("search-recovery-status").assertTextEquals(scene.context.getString(scene.state.message))
    }
}
