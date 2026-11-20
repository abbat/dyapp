package com.dyapp

import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.test.junit4.v2.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class PublicationAdmissionTests {
    @get:Rule
    val compose = createComposeRule()

    private val visible = mutableStateOf(true)
    private val state = PublishState(VisibilityState(0uL, false)) {}
    private val entered = CompletableDeferred<Unit>()
    private val release = CompletableDeferred<Unit>()
    private val completed = CompletableDeferred<Unit>()
    private var calls = 0
    private var attested = false
    private lateinit var confirm: () -> Unit

    private fun show() {
        compose.setContent {
            if (visible.value) {
                val scope = rememberCoroutineScope()
                confirm = {
                    launchPublication(scope, state) {
                        calls++
                        attested = state.adult
                        state.adult = false
                        state.public = false
                        entered.complete(Unit)
                        try {
                            release.await()
                        } finally {
                            completed.complete(Unit)
                        }
                    }
                }
            }
        }
    }

    private fun confirmBeforeRecomposition() {
        state.adult = true
        state.public = true
        confirm()
        assertEquals(0, calls)
        state.open = true
        state.adult = true
        state.public = true
        val captured = confirm
        captured()
        state.adult = true
        state.public = true
        captured()
        assertTrue(state.running)
        assertTrue(entered.isCompleted)
        assertTrue(attested)
        assertEquals(1, calls)
        visible.value = false
    }

    @Test
    fun completedAttemptsNeedFreshConfirmationsBeforeRetry() {
        show()
        compose.runOnIdle {
            state.open = true
            state.adult = true
            state.public = true
            confirm()
            assertTrue(entered.isCompleted)
            release.complete(Unit)
        }
        compose.waitUntil { completed.isCompleted }
        compose.runOnIdle {
            assertFalse(state.running)
            confirm()
            assertEquals(1, calls)
            state.adult = true
            state.public = true
            confirm()
            assertEquals(2, calls)
            assertFalse(state.running)
        }
    }

    @Test
    fun capturedCallbacksAdmitOnceBeforeRecompositionAndRejectDisposedOwners() {
        show()
        compose.runOnIdle { confirmBeforeRecomposition() }
        compose.waitUntil { completed.isCompleted }
        compose.runOnIdle {
            assertFalse(state.running)
            state.open = true
            state.adult = true
            state.public = true
            confirm()
            assertEquals(1, calls)
        }
    }
}
