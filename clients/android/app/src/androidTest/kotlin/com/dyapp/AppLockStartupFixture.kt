package com.dyapp

import android.os.Looper
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import kotlinx.coroutines.flow.MutableStateFlow
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

internal class AppLockReadGate(private val mode: StoredLock) : AutoCloseable {
    val entered = MutableStateFlow(false)
    val finished = MutableStateFlow(false)
    private val released = CountDownLatch(1)

    fun read(): StoredLock {
        check(Looper.myLooper() != Looper.getMainLooper()) { "App-lock read reached UI" }
        entered.value = true
        try {
            check(released.await(5, TimeUnit.SECONDS)) { "App-lock read fixture was not released" }
            return mode
        } finally {
            finished.value = true
        }
    }

    override fun close() = released.countDown()
}

internal class AppLockStartupFixture(private val compose: ComposeContentTestRule) {
    val modes = MutableStateFlow(emptyList<StoredLock>())
    private val shown = mutableStateOf<AppLockReadGate?>(null)

    init {
        compose.setContent {
            shown.value?.let { gate ->
                AppLockStartup(gate, gate::read, { modes.value += it }) {
                    Text("ready", Modifier.testTag("initialized-content"))
                }
            }
        }
    }

    fun show(gate: AppLockReadGate) {
        compose.runOnIdle { shown.value = gate }
        compose.awaitSnapshot(gate.entered, 5_000, "app-lock read entry") { it }
    }

    fun initialized() = compose.awaitSnapshot(modes, 5_000, "app-lock initialization") { it.isNotEmpty() }
}
