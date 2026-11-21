package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** The real dialog/actions/state with an observable outer read job and a controlled native seam. */
internal class ForwardCancelDialogFixture(private val compose: ComposeContentTestRule) {
    val state = ChatForwardPresentation().apply { ask(forwardSource()) }
    private val original = checkNotNull(state.opening)
    val entered = CompletableFuture<Unit>()
    val completed = CompletableFuture<Unit>()
    val response = CompletableFuture<List<String>>()
    val reads = AtomicInteger()
    val sends = AtomicInteger()
    private val calls = ChatForwardCalls(
        {
            if (reads.incrementAndGet() == 1) {
                entered.complete(Unit)
                response.get(60, TimeUnit.SECONDS)
            } else {
                listOf("fresh")
            }
        },
        { _, _ ->
            sends.incrementAndGet()
            ChatSubmission.Accepted(ByteArray(16))
        }
    )

    fun show() = compose.setContent {
        MaterialTheme {
            state.opening?.let { shown ->
                key(shown) {
                    val context = remember { ChatForwardContext("owner", "source", state, calls) }
                    val scope = rememberCoroutineScope()
                    val actions = remember { ChatForwardActions(context, shown, scope) }
                    LaunchedEffect(shown) {
                        if (shown === original) {
                            currentCoroutineContext().job.invokeOnCompletion { completed.complete(Unit) }
                        }
                        state.refresh(shown, calls)
                    }
                    ChatForwardDialog(state, actions)
                }
            }
        }
    }

    fun release() {
        response.complete(listOf("obsolete"))
        if (entered.isDone) compose.waitUntil(60_000) { completed.isDone }
    }
}
