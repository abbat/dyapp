package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
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

    /** A late cleanup never refreshes another identity or interrupts a pending native action. */
    suspend fun devicesCleaned(
        events: List<DeviceCleanupEvent>,
        action: () -> Opened,
        complete: (Shown) -> Unit
    ) {
        val opened = current.first { !it.loading }.shown?.opened as? Opened.Ready ?: return
        val event = events.lastOrNull { it.slotId == opened.slot.id && it.peerId == opened.peerId } ?: return
        dispatch(action, complete, event.durabilityDetail).join()
    }

    private fun dispatch(action: () -> Opened, complete: (Shown) -> Unit, detail: String? = null): Job {
        current.value = current.value.copy(loading = true)
        return scope.launch {
            val resolved = withContext(Dispatchers.IO) { resolve(action) }
            val shown = if (detail == null) resolved else Shown(resolved.opened, resolved.entries, detail)
            currentCoroutineContext().ensureActive()
            current.value = IdentityUiState(shown, false, current.value.revision + 1)
            complete(shown)
        }
    }
}
