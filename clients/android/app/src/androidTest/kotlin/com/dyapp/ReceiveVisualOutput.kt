package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Preserve partial native viewport frames with exact producer SHA and actual configuration. */
internal class ReceiveVisualOutput(private val source: String, private val width: Int) {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.filesDir, "receive-visual/$width")
    private val frames = JSONArray()

    init {
        require(Regex("[0-9a-f]{40}").matches(source))
        require(width in listOf(360, 412))
        check(!directory.exists()) { "Use a fresh test device for receiving visual evidence" }
        check(directory.mkdirs())
        writeManifest(false)
    }

    fun capture(host: ReceiveVisualHost, scene: ReceiveVisualScene, edge: String) {
        val language = scene.context.resources.configuration.locales[0].language
        val theme = if (scene.dark) "dark" else "light"
        val font = if (scene.fontScale == 2f) 200 else 100
        val name = "${scene.state.screen}-$language-$width-$theme-$font-$edge.png"
        val target = File(directory, name)
        check(target.createNewFile()) { "Duplicate receiving frame: $name" }
        val bitmap = host.compose.onNodeWithTag("settings-receive").captureToImage().asAndroidBitmap()
        try {
            target.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            frames.put(
                JSONObject().put("file", name).put("screen", scene.state.screen).put("edge", edge)
                    .put("language", language)
                    .put("theme", theme)
                    .put("font_percent", font)
                    .put("viewport_width_dp", scene.context.resources.configuration.screenWidthDp)
                    .put("viewport_height_dp", scene.context.resources.configuration.screenHeightDp)
                    .put("pixel_width", bitmap.width)
                    .put("pixel_height", bitmap.height)
            )
        } finally {
            bitmap.recycle()
        }
        writeManifest(false)
    }

    fun finish() {
        val expected = ReceiveVisualState.entries.size * 2 * 2 * 2 * 2
        check(frames.length() == expected) { "Incomplete receiving capture: ${frames.length()} != $expected" }
        writeManifest(true)
    }

    private fun writeManifest(complete: Boolean) {
        val value = JSONObject().put("source_sha", source).put("requested_width_dp", width)
            .put("complete", complete)
            .put("scope", "controlled native rendering; not authorization or transfer")
            .put("keyboard_scope", "keyboard-disabled baseline")
            .put("frames", frames)
        File(directory, "manifest.json").writeText(value.toString(2) + "\n")
    }
}
