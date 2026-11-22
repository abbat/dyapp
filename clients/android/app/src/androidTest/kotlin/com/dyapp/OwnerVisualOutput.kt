package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Density
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Source metadata is supplied by the trusted producer-artifact workflow, not inferred from a UI label. */
internal class OwnerVisualOutput(private val source: String, private val width: Int) {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.filesDir, "owner-approval-visual/$width")
    private val cases = JSONArray()

    init {
        require(Regex("[0-9a-f]{40}").matches(source))
        require(width in listOf(360, 412))
        check(!directory.exists()) { "Visual evidence already exists; use a fresh test device" }
        check(directory.mkdirs())
        writeManifest(false)
    }

    fun capture(host: OwnerVisualHost, scene: OwnerVisualScene, edge: String) {
        val language = scene.context.resources.configuration.locales[0].language
        val theme = if (scene.dark) "dark" else "light"
        val scale = if (scene.fontScale == 2f) 200 else 100
        val name = "${scene.backend.state.screen}-$language-$width-$theme-$scale-$edge.png"
        val density = NativeVisualGeometry(host.compose, "owner-dialog").density(scene.fontScale)
        val target = File(directory, name)
        check(target.createNewFile()) { "Duplicate native frame: $name" }
        val node = host.compose.onNodeWithTag("owner-dialog")
        val bounds = node.getUnclippedBoundsInRoot()
        val bitmap = node.captureToImage().asAndroidBitmap()
        try {
            target.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            cases.put(
                metadata(scene, name, edge, density)
                    .put("language", language)
                    .put("theme", theme)
                    .put("dialog_left_dp", bounds.left.value)
                    .put("dialog_top_dp", bounds.top.value)
                    .put("dialog_right_dp", bounds.right.value)
                    .put("dialog_bottom_dp", bounds.bottom.value)
                    .put("pixel_width", bitmap.width)
                    .put("pixel_height", bitmap.height)
            )
        } finally {
            bitmap.recycle()
        }
        writeManifest(false)
    }

    private fun metadata(scene: OwnerVisualScene, name: String, edge: String, density: Density): JSONObject =
        JSONObject().put("file", name).put("screen", scene.backend.state.screen)
            .put("edge", edge)
            .put("font_percent", (scene.fontScale * 100).toInt())
            .put("actual_font_scale", density.fontScale)
            .put("actual_density", density.density)
            .put("viewport_width_dp", scene.context.resources.configuration.screenWidthDp)
            .put("viewport_height_dp", scene.context.resources.configuration.screenHeightDp)

    fun finish() {
        val expected = OwnerVisualState.entries.size * 2 * 2 * 2 * 2
        check(cases.length() == expected) { "Incomplete native capture: ${cases.length()} != $expected" }
        writeManifest(true)
    }

    private fun writeManifest(complete: Boolean) {
        val data = JSONObject().put("source_sha", source).put("requested_width_dp", width)
            .put("complete", complete)
            .put("scope", "controlled native rendering; not authorization or transfer")
            .put("keyboard_scope", "keyboard-disabled baseline; real IME review remains separate")
            .put("frames", cases)
        File(directory, "manifest.json").writeText(data.toString(2) + "\n")
    }
}
