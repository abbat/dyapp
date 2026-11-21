package com.dyapp

import androidx.compose.ui.test.junit4.ComposeTestRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout

private const val SEARCH_RUN_DEADLINE_MS = 60_000L

internal class SearchRunGate(val admitted: Result<SearchIntent>) {
    val ready = CompletableDeferred<Unit>()
    val result = CompletableDeferred<SearchState>()
    val completed = CompletableDeferred<Unit>()
    var observed: Result<SearchIntent>? = null
}

/** Native-like work can finish after cancellation; only actual job completion releases assertions. */
internal class SearchRunFixture(private val compose: ComposeTestRule) : AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val filters = SearchFilters(emptyList()).apply {
        offered = listOf(19u)
        interest = "original"
    }
    val calls = mutableListOf<SearchRunGate>()
    val presentation = SearchPresentation(scope, filters) { admitted ->
        val gate = SearchRunGate(admitted)
        calls += gate
        currentCoroutineContext().job.invokeOnCompletion { gate.completed.complete(Unit) }
        withContext(NonCancellable) {
            gate.ready.await()
            gate.observed = gate.admitted
            gate.result.await()
        }
    }

    fun finish(gate: SearchRunGate, result: SearchState) {
        gate.ready.complete(Unit)
        gate.result.complete(result)
        runBlocking { withTimeout(SEARCH_RUN_DEADLINE_MS) { gate.completed.await() } }
        compose.waitForIdle()
    }

    override fun close() {
        compose.runOnIdle {
            presentation.close()
            scope.cancel()
        }
        calls.forEach { gate -> finish(gate, SearchState.Failed(R.string.search_failed, "fixture cleanup")) }
    }
}
