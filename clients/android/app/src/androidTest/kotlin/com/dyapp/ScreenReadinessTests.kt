package com.dyapp

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.test.junit4.v2.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test

class ScreenReadinessTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun snapshotWaitRunsTheQueuedComposeContinuation() {
        val released = CompletableDeferred<Unit>()
        val entered = MutableStateFlow(false)
        val completed = MutableStateFlow(false)
        compose.setContent {
            LaunchedEffect(Unit) {
                entered.value = true
                released.await()
                completed.value = true
            }
        }
        compose.awaitSnapshot(entered, 5_000, "effect entry") { it }
        compose.mainClock.autoAdvance = false
        try {
            released.complete(Unit)
            assertFalse(completed.value)
        } finally {
            compose.mainClock.autoAdvance = true
        }
        assertEquals(true, compose.awaitSnapshot(completed, 5_000, "released effect") { it })
    }
}
