package com.dyapp

import android.graphics.RectF
import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.test.ComposeTimeoutException
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.core.os.BundleCompat
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.math.roundToInt

/** Accessibility supplies actual on-screen glyph rectangles, including the editor's own scrolling. */
internal class OwnerImeCharacters(private val compose: ComposeContentTestRule, private val window: OwnerImeWindow) {
    private var observation = "not requested"

    fun requireVisible(index: Int): RectF {
        var result: RectF? = null
        var glyph: RectF? = null
        var visible = RectF()
        try {
            compose.waitUntil(10_000) {
                glyph = character(index)
                visible = RectF(window.visibleFrame())
                val expected = compose.onNodeWithTag("owner-code-input").textLayout().getBoundingBox(index)
                observation += " expectedGlyph=${expected.width}x${expected.height}"
                result = glyph?.takeIf {
                    !it.isEmpty && visible.contains(it) &&
                        it.width().roundToInt() >= expected.width.roundToInt() &&
                        it.height().roundToInt() >= expected.height.roundToInt()
                }?.let(::RectF)
                result != null
            }
        } catch (failure: ComposeTimeoutException) {
            throw AssertionError(
                "IME glyph $index is unavailable or clipped: glyph=$glyph visible=$visible; $observation",
                failure
            )
        }
        return checkNotNull(result)
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
                observation += " selection=${node.textSelectionStart}..${node.textSelectionEnd}"
                return node
            }
            for (index in 0 until node.childCount) node.getChild(index)?.let(pending::addLast)
        }
        observation += " matching editor absent"
        return null
    }
}
