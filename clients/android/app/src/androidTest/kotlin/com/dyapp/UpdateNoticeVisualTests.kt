package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** CI-only source-bound dialog matrix, independent of real update publication or activation. */
class UpdateNoticeVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesEveryNoticeOutcomeWithRealNativeTextScale() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run through the dedicated native visual phase", arguments.getString("updatesVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val expected = UpdateNoticeVisualState.entries.size * 2 * 2 * 2 * 2
        val output = NativeScreenVisualOutput(source, width, "update-notice-visual", expected)
        val host = UpdateNoticeVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (dark in listOf(false, true)) captureFontCases(host, output, language, dark)
        }
        output.finish()
    }

    private fun captureFontCases(
        host: UpdateNoticeVisualHost,
        output: NativeScreenVisualOutput,
        language: String,
        dark: Boolean,
    ) {
        for (font in listOf(1f, 2f)) {
            val context = ownerVisualContext(language, font)
            for (state in UpdateNoticeVisualState.entries) {
                capture(host, output, UpdateNoticeVisualScene(state, context, dark, font))
                progress.completed()
            }
        }
    }

    private fun capture(
        host: UpdateNoticeVisualHost,
        output: NativeScreenVisualOutput,
        scene: UpdateNoticeVisualScene,
    ) {
        try {
            host.show(scene)
            compose.awaitNode(hasTestTag(scene.metadata.rootTag))
            if (scene.state.permission) preparePermission(scene)
            if (scene.state == UpdateNoticeVisualState.OPEN_ERROR) {
                compose.onNodeWithTag("update-notice-open").performClick()
                val message = scene.context.getString(R.string.update_open_failed)
                compose.awaitNode(hasTestTag("update-notice-status") and hasText(message))
            }
            checkGeometry(output, scene)
        } finally {
            host.hide()
        }
    }

    private fun checkGeometry(output: NativeScreenVisualOutput, scene: UpdateNoticeVisualScene) {
        val geometry = NativeVisualGeometry(compose, scene.metadata.rootTag)
        geometry.first()
        output.capture(compose, scene.metadata, "top")
        geometry.check()
        checkActions(scene)
        output.capture(compose, scene.metadata, "bottom")
    }

    private fun preparePermission(scene: UpdateNoticeVisualScene) {
        compose.awaitNode(hasTestTag("update-notice-permission"))
        if (scene.state == UpdateNoticeVisualState.PERMISSION_PENDING ||
            scene.state == UpdateNoticeVisualState.PERMISSION_FAILED
        ) {
            NativeVisualGeometry(compose, scene.metadata.rootTag).check()
            compose.onNodeWithTag("update-notice-allow").performClick()
            compose.runOnIdle {
                val expected = if (scene.state == UpdateNoticeVisualState.PERMISSION_PENDING) 1 else 0
                check(scene.registry.requests.size == expected)
            }
        }
        if (scene.state == UpdateNoticeVisualState.PERMISSION_FAILED) {
            compose.awaitNode(hasText(scene.context.getString(R.string.update_notice_permission_failed)))
        }
    }

    private fun checkActions(scene: UpdateNoticeVisualScene) {
        if (scene.state.permission) {
            compose.onNodeWithTag("update-notice-allow").assertIsDisplayed()
            if (scene.state == UpdateNoticeVisualState.PERMISSION_PENDING) {
                compose.onNodeWithTag("update-notice-allow").assertIsNotEnabled()
            }
            return
        }
        compose.onNodeWithTag("update-notice-open").assertIsDisplayed()
        compose.onNodeWithTag("update-notice-later").assertIsDisplayed()
        if (scene.state == UpdateNoticeVisualState.SAVING) {
            compose.onNodeWithTag("update-notice-open").assertIsNotEnabled()
            compose.onNodeWithTag("update-notice-later").assertIsNotEnabled()
        }
    }
}
