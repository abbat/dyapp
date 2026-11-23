package com.dyapp

import androidx.compose.ui.test.assertHasNoClickAction
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** CI-only source-bound host-caption matrix, separate from ordinary JNI/Compose regressions. */
class ChatLinksVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesLiveEditedAndDeletedDestinationsInRealChatLayout() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run through the dedicated native visual phase", arguments.getString("linksVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val expected = ChatLinksVisualState.entries.size * 2 * 2 * 2 * 2
        val output = NativeScreenVisualOutput(source, width, "chat-links-visual", expected)
        val host = ChatLinksVisualHost(compose)
        for (language in listOf("en", "ru")) {
            for (dark in listOf(false, true)) captureFonts(host, output, language, dark)
        }
        output.finish()
    }

    private fun captureFonts(
        host: ChatLinksVisualHost,
        output: NativeScreenVisualOutput,
        language: String,
        dark: Boolean,
    ) {
        for (font in listOf(1f, 2f)) {
            val context = ownerVisualContext(language, font)
            for (state in ChatLinksVisualState.entries) {
                capture(host, output, ChatLinksVisualScene(state, context, dark, font))
                progress.completed()
            }
        }
    }

    private fun capture(host: ChatLinksVisualHost, output: NativeScreenVisualOutput, scene: ChatLinksVisualScene) {
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

    private fun ready(scene: ChatLinksVisualScene) {
        val caption = compose.onNodeWithTag("chat-link-hosts", useUnmergedTree = true)
        if (scene.state == ChatLinksVisualState.DELETED) {
            caption.assertDoesNotExist()
        } else {
            compose.awaitNode(hasTestTag("chat-link-hosts"))
            val hosts = if (scene.state == ChatLinksVisualState.LIVE) {
                "evil.example, xn--bcher-kva.example:8443"
            } else {
                "changed.example"
            }
            caption.assertTextEquals(scene.context.getString(R.string.chat_link_hosts, hosts))
                .assertHasNoClickAction()
        }
    }
}
