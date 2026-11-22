package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestName

/** Dedicated native visual phase; ordinary instrumentation never expands this matrix. */
class OwnerApprovalVisualTests {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val method = TestName()
    private val progress = OwnerVisualProgress(javaClass.name, method)

    @Test
    fun capturesEveryOwnerDialogState() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue("Run only through the dedicated native visual phase", arguments.getString("ownerVisual") == "1")
        val source = checkNotNull(arguments.getString("sourceSha"))
        val width = checkNotNull(arguments.getString("expectedWidth")).toInt()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.waitUntil(10_000) { context.resources.configuration.screenWidthDp == width }
        val output = OwnerVisualOutput(source, width)
        val host = OwnerVisualHost(compose)
        for (language in listOf("en", "ru")) captureLanguage(host, output, language)
        output.finish()
    }

    private fun captureLanguage(host: OwnerVisualHost, output: OwnerVisualOutput, language: String) {
        for (dark in listOf(false, true)) {
            for (font in listOf(1f, 2f)) {
                val context = ownerVisualContext(language, font)
                for (state in OwnerVisualState.entries) {
                    capture(host, output, OwnerVisualScene(OwnerVisualBackend(state), context, dark, font))
                    progress.completed()
                }
            }
        }
    }

    private fun capture(host: OwnerVisualHost, output: OwnerVisualOutput, scene: OwnerVisualScene) {
        try {
            host.show(scene)
            OwnerVisualDriver(compose).show(scene.backend)
            checkOwnerVisualState(compose, scene.backend.state)
            awaitSafeFocus(scene.backend.state)
            val geometry = NativeVisualGeometry(compose, "owner-dialog")
            geometry.first()
            output.capture(host, scene, "top")
            geometry.check()
            output.capture(host, scene, "bottom")
        } finally {
            try {
                host.hide(scene)
            } finally {
                scene.backend.close()
            }
        }
    }

    private fun awaitSafeFocus(state: OwnerVisualState) {
        if (state == OwnerVisualState.SENDING) return
        val tag = if (state == OwnerVisualState.SENT) "owner-done" else "owner-cancel"
        compose.awaitNode(hasTestTag(tag) and isFocused())
    }
}
