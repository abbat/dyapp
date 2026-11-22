package com.dyapp

import android.graphics.RectF
import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.core.os.BundleCompat
import androidx.test.platform.app.InstrumentationRegistry

/** Accessibility supplies actual on-screen glyph rectangles, including the editor's own scrolling. */
internal class OwnerImeCharacters(private val compose: ComposeContentTestRule, private val window: OwnerImeWindow) {
    fun requireVisible(index: Int): RectF {
        var result: RectF? = null
        compose.waitUntil(10_000) {
            val glyph = character(index)
            val visible = RectF(window.visibleFrame())
            if (glyph != null && !glyph.isEmpty && visible.contains(glyph)) result = RectF(glyph)
            result != null
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
        return if (editor.refreshWithExtraData(key, arguments)) {
            BundleCompat.getParcelableArray(editor.extras, key, RectF::class.java)?.firstOrNull() as? RectF
        } else {
            null
        }
    }

    private fun editor(): AccessibilityNodeInfo? {
        val root = InstrumentationRegistry.getInstrumentation().uiAutomation.rootInActiveWindow
        val pending = ArrayDeque<AccessibilityNodeInfo>()
        root?.let(pending::addLast)
        while (pending.isNotEmpty()) {
            val node = pending.removeFirst()
            if (node.isEditable && node.text?.toString() == OWNER_VISUAL_CODE) return node
            for (index in 0 until node.childCount) node.getChild(index)?.let(pending::addLast)
        }
        return null
    }
}
