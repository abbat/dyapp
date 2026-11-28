package com.dyapp

import android.graphics.RectF
import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ComposeTimeoutException
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.core.os.BundleCompat
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.math.roundToInt

/** Accessibility supplies actual on-screen glyph rectangles, including the editor's own scrolling. */
internal class OwnerImeCharacters(private val compose: ComposeContentTestRule, private val window: OwnerImeWindow) {
    private var observation = "not requested"

    fun select(index: Int) {
        val arguments = Bundle().apply {
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, index)
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, index)
        }
        try {
            compose.waitUntil(10_000) {
                editor()?.takeIf(AccessibilityNodeInfo::isFocused)
                    ?.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, arguments) == true
            }
        } catch (failure: ComposeTimeoutException) {
            throw AssertionError("Android did not accept selection $index: $observation", failure)
        }
    }

    fun requireVisible(index: Int): RectF {
        var result: RectF? = null
        var glyph: RectF? = null
        var visible = RectF()
        try {
            compose.waitUntil(10_000) {
                glyph = character(index)
                visible = RectF(window.visibleFrame())
                val expected = expectedGlyph(index)
                result = glyph?.takeIf {
                    !it.isEmpty && visible.contains(it) &&
                        it.width().roundToInt() >= expected.width.roundToInt() &&
                        it.height().roundToInt() >= expected.height.roundToInt()
                }?.let(::RectF)
                result != null
            }
        } catch (failure: ComposeTimeoutException) {
            val current = observation
            val lineSamples = sampleLineStarts()
            observation = "$current lineStartScreenBounds=$lineSamples ${scrollPosition()}"
            throw AssertionError(
                "IME glyph $index is unavailable or clipped: glyph=$glyph visible=$visible; $observation",
                failure
            )
        }
        return checkNotNull(result)
    }

    private fun sampleLineStarts(): List<RectF?> {
        val layout = compose.onNodeWithTag("owner-code-input").textLayout()
        return (0 until layout.lineCount).map { character(layout.getLineStart(it)) }
    }

    private fun scrollPosition(): String = listOf("owner-code-input", "owner-body").joinToString { tag ->
        val node = compose.onNodeWithTag(tag).fetchSemanticsNode()
        compose.runOnIdle {
            val range = node.config.getOrNull(SemanticsProperties.VerticalScrollAxisRange)
            "$tag scroll=${range?.value?.invoke()}/${range?.maxValue?.invoke()} bounds=${node.boundsInWindow}"
        }
    }

    private fun expectedGlyph(index: Int): androidx.compose.ui.geometry.Rect {
        val layout = compose.onNodeWithTag("owner-code-input").textLayout()
        val expected = layout.getBoundingBox(index)
        val line = layout.getLineForOffset(index)
        observation += " expectedGlyph=$expected line=$line start=${layout.getLineStart(line)}"
        observation += " caret=${layout.getCursorRect(index)}"
        if (index > 0) observation += " previousLine=${layout.getLineForOffset(index - 1)}"
        return expected
    }

    private fun character(index: Int): RectF? {
        val editor = editor() ?: return null
        val arguments = Bundle().apply {
            putInt(AccessibilityNodeInfo.EXTRA_DATA_TEXT_CHARACTER_LOCATION_ARG_START_INDEX, index)
            putInt(AccessibilityNodeInfo.EXTRA_DATA_TEXT_CHARACTER_LOCATION_ARG_LENGTH, 1)
        }
        val key = AccessibilityNodeInfo.EXTRA_DATA_TEXT_CHARACTER_LOCATION_KEY
        val refreshed = editor.refreshWithExtraData(key, arguments)
        observation += " extraData=$refreshed"
        observation += " refreshedSelection=${editor.textSelectionStart}..${editor.textSelectionEnd}"
        return if (refreshed) {
            BundleCompat.getParcelableArray(editor.extras, key, RectF::class.java)?.firstOrNull() as? RectF
        } else {
            null
        }
    }

    private fun editor(): AccessibilityNodeInfo? {
        val root = InstrumentationRegistry.getInstrumentation().uiAutomation.rootInActiveWindow
        observation = "activeRoot=${root?.className} package=${root?.packageName} window=${root?.windowId}"
        val pending = ArrayDeque<AccessibilityNodeInfo>()
        root?.let(pending::addLast)
        while (pending.isNotEmpty()) {
            val node = pending.removeFirst()
            if (node.isEditable && node.text?.toString() == OWNER_VISUAL_CODE) {
                observation += " editor=${node.className} focused=${node.isFocused} visible=${node.isVisibleToUser}"
                observation += " contentDescription=${node.contentDescription} hint=${node.hintText}"
                val bounds = android.graphics.Rect()
                node.getBoundsInScreen(bounds)
                observation += " bounds=$bounds selection=${node.textSelectionStart}..${node.textSelectionEnd}"
                val characterLocationKey = AccessibilityNodeInfo.EXTRA_DATA_TEXT_CHARACTER_LOCATION_KEY
                observation += " characterLocationAdvertised=${characterLocationKey in node.availableExtraData}"
                return node
            }
            for (index in 0 until node.childCount) node.getChild(index)?.let(pending::addLast)
        }
        observation += " matching editor absent"
        return null
    }
}
