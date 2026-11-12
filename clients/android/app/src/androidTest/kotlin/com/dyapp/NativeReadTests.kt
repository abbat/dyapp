package com.dyapp

import android.os.Looper
import androidx.compose.material3.Text
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

private class NativeReadGate(private val value: String = "old") {
    val entered = CompletableFuture<Unit>()
    val finished = CompletableFuture<Unit>()
    private val released = CountDownLatch(1)

    fun read(): String {
        check(Looper.myLooper() != Looper.getMainLooper()) { "Read reached UI thread" }
        entered.complete(Unit)
        try {
            check(released.await(5, TimeUnit.SECONDS)) { "Native read fixture was not released" }
            return value
        } finally {
            finished.complete(Unit)
        }
    }

    fun release() = released.countDown()
}

class NativeReadTests {
    @get:Rule
    val compose = createComposeRule()

    // The test dispatcher starts a new key's read only while compose idles, so never block.
    private fun await(step: CompletableFuture<Unit>) = compose.waitUntil(5_000) { step.isDone }

    @Test
    fun replacementOwnerStartsEmptyAndRejectsTheBlockedOwnersLateResult() {
        val old = NativeReadGate()
        val next = NativeReadGate("new")
        var owner by mutableStateOf(old)
        var latest: State<Result<String>?>? = null
        try {
            compose.setContent {
                val captured = owner
                val loaded = rememberNativeRead(captured) { captured.read() }
                latest = loaded
                Text(loaded.value?.getOrNull() ?: "loading")
            }
            await(old.entered)
            val oldState = compose.runOnIdle { latest!! }
            compose.runOnIdle { owner = next }
            await(next.entered)
            old.release()
            await(old.finished)
            compose.runOnIdle {
                assertNull(oldState.value)
                assertNull(latest!!.value)
            }
        } finally {
            old.release()
            next.release()
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithText("new").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("old").assertDoesNotExist()
    }

    @Test
    fun aReplacementOwnerDoesNotInheritThePreviousOwnersCompletedValue() {
        val gate = NativeReadGate("new")
        var fresh by mutableStateOf(false)
        try {
            compose.setContent {
                val selected = fresh
                val loaded by rememberNativeRead(selected) { if (selected) gate.read() else "old" }
                Text(loaded?.getOrNull() ?: "loading")
            }
            compose.waitUntil(5_000) { compose.onAllNodesWithText("old").fetchSemanticsNodes().isNotEmpty() }
            compose.runOnIdle { fresh = true }
            await(gate.entered)
            compose.onNodeWithText("loading").assertExists()
            compose.onNodeWithText("old").assertDoesNotExist()
        } finally {
            gate.release()
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithText("new").fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun leavingTheScreenDiscardsAReadThatFinishesAfterDisposal() {
        val gate = NativeReadGate()
        var visible by mutableStateOf(true)
        var loaded: State<Result<String>?>? = null
        try {
            compose.setContent {
                if (visible) {
                    val current = rememberNativeRead(gate, read = gate::read)
                    loaded = current
                    Text(current.value?.getOrNull() ?: "loading")
                } else {
                    Text("closed")
                }
            }
            await(gate.entered)
            compose.runOnIdle { visible = false }
            compose.onNodeWithText("closed").assertExists()
            gate.release()
            await(gate.finished)
            compose.runOnIdle { assertNull(loaded!!.value) }
        } finally {
            gate.release()
        }
    }

    @Test
    fun aReadFailureCompletesAsFailureRatherThanAnEmptySuccess() {
        var loaded: State<Result<String>?>? = null
        compose.setContent {
            val current = rememberNativeRead<String>(Unit) { error("denied") }
            loaded = current
            Text(if (current.value == null) "loading" else "done")
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithText("done").fetchSemanticsNodes().isNotEmpty() }
        compose.runOnIdle { assertFalse(loaded!!.value!!.isSuccess) }
    }
}
