package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** One atomically published loading/result state for the existing identity screen. */
internal data class IdentityUiState(val shown: Shown? = null, val loading: Boolean = true, val revision: Long = 0)

/** UI-thread commands dispatch blocking registry work and never publish a cancelled owner's result. */
internal class IdentityActions(
    private val scope: CoroutineScope,
    private val resolve: (() -> Opened) -> Shown
) {
    private val current = MutableStateFlow(IdentityUiState())
    val state = current.asStateFlow()

    fun initialize(action: () -> Opened, complete: (Shown) -> Unit = {}): Job = dispatch(action, complete)

    fun perform(action: () -> Opened, complete: (Shown) -> Unit = {}): Job? =
        if (current.value.loading) null else dispatch(action, complete)

    private fun dispatch(action: () -> Opened, complete: (Shown) -> Unit): Job {
        current.value = current.value.copy(loading = true)
        return scope.launch {
            val shown = withContext(Dispatchers.IO) { resolve(action) }
            currentCoroutineContext().ensureActive()
            current.value = IdentityUiState(shown, false, current.value.revision + 1)
            complete(shown)
        }
    }
}
