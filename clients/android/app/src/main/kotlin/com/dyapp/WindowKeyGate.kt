package com.dyapp

import android.view.KeyEvent
import android.view.Window

/** Consume an expired-lock key before the activity dispatches it to private content. */
internal class WindowKeyGate(
    private val callback: Window.Callback,
    private val consume: () -> Boolean,
) : Window.Callback by callback {
    override fun dispatchKeyEvent(event: KeyEvent): Boolean = consume() || callback.dispatchKeyEvent(event)
}
