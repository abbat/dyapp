package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Rule
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState

internal open class ChatReadFixture {
    @get:Rule
    val compose = createComposeRule()

    protected class Gate {
        val entered = CompletableDeferred<Unit>()
        val released = CompletableDeferred<ChatThreadSnapshot>()
        val returned = CompletableDeferred<Unit>()

        suspend fun read(): ChatThreadSnapshot {
            entered.complete(Unit)
            return try {
                // Native work may return after its caller was cancelled.
                withContext(NonCancellable) { released.await() }
            } finally {
                returned.complete(Unit)
            }
        }
    }

    protected class Selection {
        var owner by mutableStateOf("old")
        var revision by mutableIntStateOf(0)
        var shown by mutableStateOf("loading")
        val applied = CompletableDeferred<Unit>()
        val published = mutableListOf<ChatState>()
        val acknowledgementOwners = mutableListOf<String>()

        fun publish(result: Result<ChatThreadSnapshot>) {
            shown = result.fold({
                published += it.history.state
                it.history.state.name
            }, { "failed" })
        }
    }

    protected class Acknowledgement {
        val entered = CompletableDeferred<Unit>()
        val released = CompletableDeferred<Unit>()
        val again = CompletableDeferred<Unit>()
        var calls = 0

        suspend fun run() {
            calls += 1
            if (calls == 1) {
                entered.complete(Unit)
                released.await()
            } else {
                again.complete(Unit)
            }
        }
    }

    protected fun snapshot(state: ChatState) = ChatThreadSnapshot(ChatHistory(state, emptyList(), false), emptyList())

    protected fun await(signal: Deferred<Unit>) = compose.waitUntil(CHECK_WAIT_MS) { signal.isCompleted }

    protected fun show(
        selection: Selection,
        read: suspend () -> ChatThreadSnapshot,
        acknowledge: suspend () -> Unit,
    ) {
        compose.setContent {
            SideEffect { if (selection.revision == 1) selection.applied.complete(Unit) }
            ChatReadEffects(selection.owner to selection.revision, read, selection::publish) {
                selection.acknowledgementOwners += selection.owner
                acknowledge()
            }
            Text(selection.shown)
        }
    }

    protected fun refreshDuringRead(selection: Selection, first: Gate, second: Gate) {
        await(first.entered)
        compose.runOnIdle { selection.revision += 1 }
        await(selection.applied)
        first.released.complete(snapshot(ChatState.REQUEST_RECEIVED))
        await(first.returned)
        await(second.entered)
    }

    protected fun release(first: Gate, second: Gate, acknowledgement: Acknowledgement) {
        first.released.complete(snapshot(ChatState.OPEN))
        second.released.complete(snapshot(ChatState.OPEN))
        acknowledgement.released.complete(Unit)
    }
}
