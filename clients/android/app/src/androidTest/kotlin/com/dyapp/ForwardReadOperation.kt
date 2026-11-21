package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import uniffi.dyapp.ChatSubmission
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

internal class ForwardReadOperation(state: ChatForwardPresentation, shown: ForwardSelection, scope: CoroutineScope) {
    private val entered = CompletableDeferred<Unit>()
    private val response = CompletableFuture<List<String>>()
    val sends = AtomicInteger()
    private val calls = ChatForwardCalls(
        {
            entered.complete(Unit)
            response.get(60, TimeUnit.SECONDS)
        },
        { _, _ ->
            sends.incrementAndGet()
            ChatSubmission.Accepted(ByteArray(16))
        }
    )
    private val job = scope.launch(start = CoroutineStart.UNDISPATCHED) { state.refresh(shown, calls) }

    suspend fun awaitStarted() = withTimeout(60_000) { entered.await() }

    suspend fun complete(targets: List<String>) {
        response.complete(targets)
        withTimeout(60_000) { job.join() }
    }

    suspend fun fail() {
        response.completeExceptionally(IllegalStateException("controlled old read failure"))
        withTimeout(60_000) { job.join() }
    }

    suspend fun close() {
        response.complete(emptyList())
        job.cancelAndJoin()
    }
}
