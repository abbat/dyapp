package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import kotlinx.coroutines.awaitCancellation
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList

class ChatPresenceForegroundTests {
    @get:Rule
    val compose = createComposeRule()

    private class Owner : LifecycleOwner {
        val registry = LifecycleRegistry(this)
        override val lifecycle: Lifecycle get() = registry
    }

    private val owner = Owner()

    private fun state(value: Lifecycle.State) = compose.runOnIdle { owner.registry.currentState = value }

    private fun showAttempts(
        identity: MutableState<String>,
        revision: MutableState<Int>,
        visible: MutableState<Boolean>,
        calls: MutableList<String>
    ) = compose.setContent {
        Text(revision.value.toString())
        if (visible.value) {
            val selected = identity.value
            ChatForegroundEffect(owner, selected) { calls += selected }
        }
    }

    private fun showPending(
        identity: MutableState<String>,
        entered: MutableList<String>,
        retired: MutableList<String>
    ) = compose.setContent {
        val selected = identity.value
        ChatForegroundEffect(owner, selected) {
            entered += selected
            try {
                awaitCancellation()
            } finally {
                retired += selected
            }
        }
    }

    @Test
    fun onlyResumeAndIdentityOpeningTriggerAnAttempt() {
        val identity = mutableStateOf("first")
        val revision = mutableStateOf(0)
        val visible = mutableStateOf(true)
        val calls = CopyOnWriteArrayList<String>()
        state(Lifecycle.State.CREATED)
        showAttempts(identity, revision, visible, calls)
        state(Lifecycle.State.STARTED)
        compose.runOnIdle { assertEquals(emptyList<String>(), calls) }
        state(Lifecycle.State.RESUMED)
        compose.waitUntil { calls.size == 1 }
        compose.runOnIdle { revision.value += 1 }
        compose.runOnIdle { assertEquals(listOf("first"), calls) }
        state(Lifecycle.State.STARTED)
        compose.runOnIdle { assertEquals(listOf("first"), calls) }
        state(Lifecycle.State.RESUMED)
        compose.waitUntil { calls.size == 2 }
        compose.runOnIdle { identity.value = "second" }
        compose.waitUntil { calls.size == 3 }
        compose.runOnIdle {
            assertEquals(listOf("first", "first", "second"), calls)
            visible.value = false
        }
        state(Lifecycle.State.STARTED)
        state(Lifecycle.State.RESUMED)
        compose.runOnIdle { assertEquals(3, calls.size) }
    }

    @Test
    fun pauseAndReplacementCancelTheOldEventBeforeAnotherOpening() {
        val identity = mutableStateOf("first")
        val entered = CopyOnWriteArrayList<String>()
        val retired = CopyOnWriteArrayList<String>()
        state(Lifecycle.State.CREATED)
        showPending(identity, entered, retired)
        state(Lifecycle.State.RESUMED)
        compose.waitUntil { entered.size == 1 }
        state(Lifecycle.State.STARTED)
        compose.waitUntil { retired.size == 1 }
        state(Lifecycle.State.RESUMED)
        compose.waitUntil { entered.size == 2 }
        compose.runOnIdle { identity.value = "second" }
        compose.waitUntil { retired.size == 2 && entered.size == 3 }
        compose.runOnIdle {
            assertEquals(listOf("first", "first"), retired)
            assertEquals(listOf("first", "first", "second"), entered)
        }
        state(Lifecycle.State.DESTROYED)
        compose.waitUntil { retired.size == 3 }
    }
}
