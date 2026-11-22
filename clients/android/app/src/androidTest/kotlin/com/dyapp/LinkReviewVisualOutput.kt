package com.dyapp

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.unit.Density
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Each completed scene proves that saved viewports contain every required actual value line. */
internal class LinkReviewVisualOutput(
    private val compose: ComposeContentTestRule,
    private val source: String,
    private val width: Int,
    private val language: String
) {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.filesDir, "comparison-visual/$width/$language")
    private val coverage = LinkReviewVisualCoverage(compose)
    private val scenes = JSONArray()
    private val covered = mutableMapOf<String, MutableSet<Int>>()
    private var required = emptyMap<String, List<Int>>()
    private var frames = JSONArray()
    private var name = ""
    private var density = Density(1f)

    init {
        require(Regex("[0-9a-f]{40}").matches(source))
        require(width in listOf(360, 412) && language in listOf("en", "ru"))
        check(!directory.exists() && directory.mkdirs()) { "Use a fresh comparison visual device" }
        writeManifest(false)
    }

    fun start(state: LinkReviewVisualState, expected: Set<String>, font: Float, dark: Boolean) {
        required = coverage.required()
        check(required.keys == expected) { "The actual comparison must expose every offered value" }
        covered.clear()
        frames = JSONArray()
        val theme = if (dark) "dark" else "light"
        name = "${state.name.lowercase()}-$language-$width-$theme-${(font * 100).toInt()}"
        density = NativeVisualGeometry(compose, "settings-receive").density(font)
        scenes.put(JSONObject().put("scene", name).put("required", JSONObject(required)).put("frames", frames))
    }

    fun capturePhotos(state: LinkReviewVisualState) {
        val sources = if (state == LinkReviewVisualState.PREVIEW) listOf("REMOTE") else listOf("LOCAL", "REMOTE")
        val suffix = if (state == LinkReviewVisualState.PHOTO_UNAVAILABLE) "-retry" else ""
        for (source in sources) {
            val tag = "link-review-photo-$source-0$suffix"
            val node = compose.onNodeWithTag(tag).performScrollTo().assertIsDisplayed()
            val bounds = node.getUnclippedBoundsInRoot()
            val viewport = compose.onNodeWithTag("settings-receive").getUnclippedBoundsInRoot()
            check(bounds.left >= viewport.left && bounds.right <= viewport.right)
            check(bounds.top >= viewport.top && bounds.bottom <= viewport.bottom)
            save(tag)
        }
    }

    fun capture() = save(null)

    private fun save(photo: String?) {
        val visible = coverage.visible().filterValues { it.isNotEmpty() }
        for ((tag, lines) in visible) covered.getOrPut(tag) { mutableSetOf() }.addAll(lines)
        val file = "$name-${frames.length()}.png"
        val target = File(directory, file)
        check(target.createNewFile()) { "Duplicate comparison frame" }
        val bitmap = compose.onNodeWithTag("settings-receive").captureToImage().asAndroidBitmap()
        try {
            target.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
            frames.put(
                JSONObject().put("file", file).put("visible", JSONObject(visible)).put("photo", photo)
                    .put("font_percent", (density.fontScale * 100).toInt())
                    .put("actual_font_scale", density.fontScale)
                    .put("actual_density", density.density)
                    .put("viewport_width_dp", context.resources.configuration.screenWidthDp)
                    .put("pixel_width", bitmap.width)
                    .put("pixel_height", bitmap.height)
            )
        } finally {
            bitmap.recycle()
        }
        writeManifest(false)
    }

    fun finishScene() {
        for ((tag, lines) in required) {
            check(covered[tag] == lines.toSet()) { "Saved comparison frames miss actual lines of $tag" }
        }
        scenes.getJSONObject(scenes.length() - 1).put("complete", true)
        writeManifest(false)
    }

    fun finish() {
        check(scenes.length() == LinkReviewVisualState.entries.size * 2 * 2)
        writeManifest(true)
    }

    private fun writeManifest(complete: Boolean) {
        val value = JSONObject().put("source_sha", source).put("requested_width_dp", width)
            .put("language", language)
            .put("complete", complete)
            .put("scenes", scenes)
            .put("scope", "controlled native comparison rendering; not transfer or authorization")
        File(directory, "manifest.json").writeText(value.toString() + "\n")
    }
}
