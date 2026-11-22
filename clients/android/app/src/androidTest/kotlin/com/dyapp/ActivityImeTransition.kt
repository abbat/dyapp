package com.dyapp

import android.app.Activity
import android.view.ViewTreeObserver
import android.view.Window
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import java.util.concurrent.atomic.AtomicBoolean

/** Hide through the actual Activity window; acknowledge focus and current insets before scrolling. */
internal fun ComposeContentTestRule.hideActivityIme(activity: Activity) {
    val transition = runOnUiThread { ActivityImeTransition(activity.window) }
    try {
        runOnUiThread { transition.hide() }
        waitUntil(CHECK_WAIT_MS) { transition.ready.get() }
    } finally {
        runOnUiThread { transition.close() }
    }
}

private class ActivityImeTransition(private val window: Window) : AutoCloseable {
    val ready = AtomicBoolean()
    private val root = window.decorView
    private val tree = root.viewTreeObserver
    private val draw = ViewTreeObserver.OnPreDrawListener {
        observe()
        true
    }
    private val focus = ViewTreeObserver.OnWindowFocusChangeListener { observe() }

    fun hide() {
        check(tree.isAlive) { "Activity window observer is unavailable" }
        tree.addOnPreDrawListener(draw)
        tree.addOnWindowFocusChangeListener(focus)
        WindowInsetsControllerCompat(window, root).hide(WindowInsetsCompat.Type.ime())
        root.requestApplyInsets()
        observe()
    }

    private fun observe() {
        val insets = ViewCompat.getRootWindowInsets(root) ?: return
        if (root.hasWindowFocus() && !root.isLayoutRequested && !insets.isVisible(WindowInsetsCompat.Type.ime())) {
            ready.set(true)
        }
    }

    override fun close() {
        if (tree.isAlive) {
            tree.removeOnPreDrawListener(draw)
            tree.removeOnWindowFocusChangeListener(focus)
        }
    }
}
