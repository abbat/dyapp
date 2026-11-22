package com.dyapp

import android.graphics.Rect
import android.graphics.RectF
import android.view.View
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.espresso.Espresso.onView
import androidx.test.espresso.matcher.RootMatchers.isDialog
import androidx.test.espresso.matcher.ViewMatchers.isRoot
import java.util.concurrent.atomic.AtomicReference

/** Native dialog coordinates and actual IME visibility, without status-bar or keyboard offsets. */
internal class OwnerImeWindow(private val compose: ComposeContentTestRule) {
    private val root: View

    init {
        val found = AtomicReference<View>()
        onView(isRoot()).inRoot(isDialog()).check { view, error ->
            if (error != null) throw error
            found.set(checkNotNull(view))
        }
        root = checkNotNull(found.get())
    }

    fun awaitVisible() {
        compose.waitUntil(10_000) {
            compose.runOnIdle {
                ViewCompat.getRootWindowInsets(root)?.isVisible(WindowInsetsCompat.Type.ime()) == true
            }
        }
    }

    fun visibleFrame(): Rect = compose.runOnIdle {
        Rect().also(root::getWindowVisibleDisplayFrame)
    }

    fun requireControl(tag: String): RectF {
        val node = compose.onNodeWithTag(tag).fetchSemanticsNode()
        var result: RectF? = null
        compose.waitUntil(10_000) {
            result = compose.runOnIdle {
                val screen = IntArray(2).also(root::getLocationOnScreen)
                val window = IntArray(2).also(root::getLocationInWindow)
                val x = screen[0] - window[0] + node.positionInWindow.x
                val y = screen[1] - window[1] + node.positionInWindow.y
                val bounds = RectF(x, y, x + node.size.width, y + node.size.height)
                val visible = RectF(Rect().also(root::getWindowVisibleDisplayFrame))
                bounds.takeIf { !it.isEmpty && visible.contains(it) }
            }
            result != null
        }
        return checkNotNull(result)
    }
}
