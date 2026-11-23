package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** CI-only native projection/audience presentation, separate from actual protocol delivery tests. */
class PresenceVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesMarksAndAudienceOutcomesWithActualNativeTextScale() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run through the dedicated native visual phase", arguments.getString("activityVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val expected = PresenceVisualState.entries.size * 2 * 2 * 2 * 2
        val output = NativeScreenVisualOutput(source, width, "activity-visual", expected)
        val host = PresenceVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (dark in listOf(false, true)) captureFonts(host, output, language, dark)
        }
        output.finish()
    }

    private fun captureFonts(
        host: PresenceVisualHost,
        output: NativeScreenVisualOutput,
        language: String,
        dark: Boolean,
    ) {
        for (font in listOf(1f, 2f)) {
            val context = ownerVisualContext(language, font)
            for (state in PresenceVisualState.entries) {
                capture(host, output, PresenceVisualScene(state, context, dark, font))
                progress.completed()
            }
        }
    }

    private fun capture(host: PresenceVisualHost, output: NativeScreenVisualOutput, scene: PresenceVisualScene) {
        try {
            host.show(scene)
            compose.awaitNode(hasTestTag(scene.metadata.rootTag))
            ready(scene)
            val geometry = NativeVisualGeometry(compose, scene.metadata.rootTag)
            geometry.first()
            output.capture(compose, scene.metadata, "top")
            geometry.check()
            output.capture(compose, scene.metadata, "bottom")
        } finally {
            host.hide()
        }
    }

    private fun ready(scene: PresenceVisualScene) {
        when (scene.state) {
            PresenceVisualState.MATCHES -> {
                compose.awaitNode(hasTestTag("presence-matches") and isSelected())
            }
            PresenceVisualState.NOBODY -> {
                compose.awaitNode(hasTestTag("presence-nobody") and isSelected())
            }
            PresenceVisualState.LOADING -> {
                compose.waitUntil(10_000) { scene.entered.isCompleted }
                compose.onNodeWithTag("presence-matches").assertIsNotEnabled()
                compose.onNodeWithTag("presence-nobody").assertIsNotEnabled()
                unknownAudience()
            }
            PresenceVisualState.ERROR -> {
                compose.awaitNode(hasTestTag("presence-error"))
                unknownAudience()
                compose.onNodeWithTag("presence-retry").assertIsEnabled()
            }
            else -> { mark(scene) }
        }
    }

    private fun unknownAudience() {
        compose.onNodeWithTag("presence-matches").assertIsNotSelected()
        compose.onNodeWithTag("presence-nobody").assertIsNotSelected()
    }

    private fun mark(scene: PresenceVisualScene) {
        val target = compose.onNodeWithTag("chat-presence-$PRESENCE_VISUAL_PEER", useUnmergedTree = true)
        if (scene.state == PresenceVisualState.LIST_ACTIVE || scene.state == PresenceVisualState.CHAT_ACTIVE) {
            target.assertTextEquals(scene.context.getString(R.string.chat_active_recently))
        } else {
            target.assertDoesNotExist()
        }
    }
}
