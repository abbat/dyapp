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
import uniffi.dyapp.MobileUpdateNoticeAction
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

internal data class UpdateNoticeUiState(
    val snapshot: MobileUpdateState? = null,
    val pending: Boolean = false,
    val failed: Boolean = false,
)

/** UI-thread admission only; eligibility and durable acknowledgement belong to core. */
internal class UpdateNoticeActions(
    private val scope: CoroutineScope,
    private val read: () -> MobileUpdateState,
    private val dismiss: (String) -> MobileUpdateNoticeAction,
) {
    private val current = MutableStateFlow(UpdateNoticeUiState())
    val state = current.asStateFlow()

    fun initialize(): Job? = if (current.value.pending) null else dispatch(read)

    fun later(expected: MobileUpdateOffer): Job? {
        if (current.value.pending || current.value.snapshot?.notice != expected) return null
        return dispatch { dismiss(expected.version).state }
    }

    private fun dispatch(action: () -> MobileUpdateState): Job {
        current.value = current.value.copy(pending = true, failed = false)
        return scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching(action) }
            currentCoroutineContext().ensureActive()
            current.value = UpdateNoticeUiState(
                snapshot = result.getOrNull() ?: current.value.snapshot,
                failed = result.isFailure,
            )
        }
    }
}
