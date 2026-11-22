package com.dyapp

import android.graphics.Bitmap
import android.graphics.RectF
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

internal data class OwnerImeMeasurement(
    val visible: RectF,
    val glyph: RectF,
    val cancel: RectF,
    val next: RectF
)

/** Dialog PNGs plus measured native IME/glyph/control coordinates; no guessed editor padding. */
internal class OwnerImeEvidence(private val source: String, private val width: Int) {
    private val directory = File(
        InstrumentationRegistry.getInstrumentation().targetContext.filesDir,
        "owner-approval-ime/$width"
    )
    private val frames = JSONArray()

    init {
        require(Regex("[0-9a-f]{40}").matches(source) && width in listOf(360, 412))
        check(!directory.exists() && directory.mkdirs()) { "Use a fresh real-IME evidence directory" }
        manifest(false)
    }

    fun capture(host: OwnerVisualHost, scene: OwnerVisualScene, index: Int, measured: OwnerImeMeasurement) {
        val language = scene.context.resources.configuration.locales[0].language
        val theme = if (scene.dark) "dark" else "light"
        val scale = if (scene.fontScale == 2f) 200 else 100
        val name = "owner-ime-$language-$width-$theme-$scale-$index.png"
        val file = File(directory, name)
        check(file.createNewFile())
        val bitmap = host.compose.onNodeWithTag("owner-dialog").captureToImage().asAndroidBitmap()
        try {
            file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
        } finally {
            bitmap.recycle()
        }
        frames.put(
            JSONObject().put("file", name).put("selection", index)
                .put("visible_screen_px", rectangle(measured.visible))
                .put("glyph_screen_px", rectangle(measured.glyph))
                .put("cancel_screen_px", rectangle(measured.cancel))
                .put("next_screen_px", rectangle(measured.next))
        )
        manifest(false)
    }

    fun finish() {
        check(frames.length() == 24) { "Incomplete real-IME matrix" }
        manifest(true)
    }

    private fun manifest(complete: Boolean) {
        val data = JSONObject().put("source_sha", source).put("width_dp", width)
            .put("complete", complete)
            .put("scope", "real IME editor geometry; controlled preparation, no native transfer")
            .put("frames", frames)
        File(directory, "manifest.json").writeText(data.toString(2) + "\n")
    }

    private fun rectangle(value: RectF): JSONArray =
        JSONArray(listOf(value.left, value.top, value.right, value.bottom))
}
