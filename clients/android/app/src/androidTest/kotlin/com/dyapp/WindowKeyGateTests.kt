package com.dyapp

import android.view.KeyEvent
import android.view.Window
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.lang.reflect.Proxy

class WindowKeyGateTests {
    @Test
    fun expiredLockConsumesTheKeyAndOtherwisePreservesTheDelegateResult() {
        var calls = 0
        var result = false
        val callback = Proxy.newProxyInstance(
            Window.Callback::class.java.classLoader,
            arrayOf(Window.Callback::class.java),
        ) { _, method, _ ->
            check(method.name == "dispatchKeyEvent")
            calls++
            result
        } as Window.Callback
        var blocked = true
        val gate = WindowKeyGate(callback) { blocked }
        val event = KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_A)
        assertTrue(gate.dispatchKeyEvent(event))
        assertEquals(0, calls)
        blocked = false
        assertFalse(gate.dispatchKeyEvent(event))
        assertEquals(1, calls)
        result = true
        assertTrue(gate.dispatchKeyEvent(event))
        assertEquals(2, calls)
    }
}
