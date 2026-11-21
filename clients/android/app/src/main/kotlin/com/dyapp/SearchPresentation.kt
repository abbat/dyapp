package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/** UI-thread admission and result ownership; native work retains its existing lifetime. */
internal class SearchPresentation(
    private val scope: CoroutineScope,
    private val filters: SearchFilters,
    private val execute: suspend (Result<SearchIntent>) -> SearchState,
) {
    var state by mutableStateOf<SearchState>(SearchState.Idle)
        private set
    private var active = true
    private var revision = Any()
    private var job: Job? = null

    fun runAction(): () -> Unit {
        val shown = revision
        return {
            if (active && revision === shown && state != SearchState.Loading) {
                val operation = Any()
                revision = operation
                state = SearchState.Loading
                val admitted = filters.captureIntent()
                job = scope.launch { complete(operation, execute(admitted)) }
            }
        }
    }

    fun cancelAction(): () -> Unit {
        val shown = revision
        return {
            if (active && revision === shown && state == SearchState.Loading) {
                revision = Any()
                job?.cancel()
                state = SearchState.Idle
            }
        }
    }

    fun close() {
        active = false
        revision = Any()
        job?.cancel()
    }

    private fun complete(operation: Any, result: SearchState) {
        if (active && revision === operation) {
            if (result is SearchState.Done) filters.offered = result.result.offered
            revision = Any()
            state = result
        }
    }
}
