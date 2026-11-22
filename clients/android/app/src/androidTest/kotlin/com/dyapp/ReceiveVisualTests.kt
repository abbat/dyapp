package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** Dedicated rendering matrix; ordinary instrumentation must not expand this expensive phase. */
class ReceiveVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesEveryReceivingAndEnrollmentState() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run only through the dedicated native visual phase", arguments.getString("receiveVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val output = ReceiveVisualOutput(source, width)
        val host = ReceiveVisualHost(compose)
        withReceiveVisualFixture { fixture ->
            for (language in listOf("en", "ru")) captureLanguage(host, output, fixture, language)
        }
        output.finish()
    }

    private fun captureLanguage(
        host: ReceiveVisualHost,
        output: ReceiveVisualOutput,
        fixture: ReceiveVisualFixture,
        language: String
    ) {
        for (dark in listOf(false, true)) {
            for (font in listOf(1f, 2f)) {
                val context = ownerVisualContext(language, font)
                for (state in ReceiveVisualState.entries) {
                    capture(host, output, ReceiveVisualScene(state, fixture, context, dark, font))
                    progress.completed()
                }
            }
        }
    }

    private fun capture(host: ReceiveVisualHost, output: ReceiveVisualOutput, scene: ReceiveVisualScene) {
        try {
            host.show(scene)
            val expected = if (scene.state.intro) "link-receive-start" else "link-receive-status"
            compose.awaitNode(hasTestTag(expected))
            val geometry = NativeVisualGeometry(compose, "settings-receive")
            geometry.first()
            output.capture(host, scene, "top")
            geometry.check()
            output.capture(host, scene, "bottom")
        } finally {
            host.hide()
        }
    }
}
