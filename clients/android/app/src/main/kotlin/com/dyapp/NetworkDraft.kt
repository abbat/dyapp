package com.dyapp

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.ConnectionAttempt
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.NetworkFile
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow

private const val CHECK_WAIT_MS = 5_000

internal enum class DraftStatus { LOADING, EDITING, SAVING, SAVED, READ_FAILED, SAVE_FAILED }

/** The network form's text and what its load, check and save last reported. */
internal class NetworkDraftState {
    var seeds by mutableStateOf("")
    var name by mutableStateOf("")
    var status by mutableStateOf(DraftStatus.LOADING)
    var checker by mutableStateOf<ConnectionCheck?>(null)
    var attempt by mutableStateOf<ConnectionAttempt?>(null)
    var checkStatus by mutableStateOf<Int?>(null)
    var checkRows by mutableStateOf<List<Pair<String, SeedRow>>>(emptyList())
    var checkError by mutableStateOf<String?>(null)

    val editable
        get() = status !in setOf(DraftStatus.LOADING, DraftStatus.SAVING, DraftStatus.READ_FAILED) &&
            checkStatus !in setOf(R.string.network_checking, R.string.network_activating)

    fun editSeeds(value: String) {
        seeds = value
        clearCheck()
    }
    fun editName(value: String) {
        name = value
        clearCheck()
    }
    private fun clearCheck() {
        checkStatus = null
        checkRows = emptyList()
        checkError = null
    }
}

/** What the network form's buttons do. */
internal class DraftActions(
    val check: () -> Unit,
    val cancel: () -> Unit,
    val activate: () -> Unit,
    val save: () -> Unit
)

/** Fills [state] from the saved draft, or else from the active network. */
internal suspend fun loadDraft(opened: Opened.Ready, state: NetworkDraftState) {
    runCatching {
        withNativeResource(opened::connectionChecker) { checker ->
            val settings = withContext(Dispatchers.IO) {
                RustBridge.loadNetwork(opened, NetworkFile.DRAFT) ?: RustBridge.loadNetwork(opened, NetworkFile.ACTIVE)
            }
            state.seeds = settings?.seeds?.joinToString("\n").orEmpty()
            state.name = settings?.name.orEmpty()
            state.checker = checker
            state.status = DraftStatus.EDITING
            try {
                awaitCancellation()
            } finally {
                state.attempt?.cancel()
                checker.cancel()
                state.checker = null
            }
        }
    }.onFailure {
        currentCoroutineContext().ensureActive()
        state.status = DraftStatus.READ_FAILED
    }
}

private suspend fun checkDraft(attempt: ConnectionAttempt, draft: NetworkSettings, state: NetworkDraftState) {
    val checked = withContext(Dispatchers.IO) {
        runCatching { attempt.run(draft, CHECK_WAIT_MS.toUInt()) }
    }
    // Cancel may arrive after native completion but before the UI receives its result.
    if (attempt.isCancelled()) {
        state.checkStatus = R.string.network_cancelled
        state.checkError = null
        return
    }
    checked.fold(onSuccess = { rows ->
        state.checkRows = draft.seeds.zip(rows)
        state.checkStatus = checkSummary(rows)
    }, onFailure = {
        Log.w("dyapp", "Connection check failed", it)
        state.checkStatus = checkFailure(it)
        state.checkError = it.toString()
    })
}

private suspend fun activateDraft(checker: ConnectionCheck, state: NetworkDraftState) {
    val activated = withContext(Dispatchers.IO) { runCatching { withSearchOutboxChange { checker.activate() } } }
    state.checkStatus = if (activated.isSuccess) R.string.network_active else R.string.network_activation_failed
    state.checkError = activated.exceptionOrNull()?.toString()
}

private suspend fun saveDraft(opened: Opened.Ready, draft: NetworkSettings, state: NetworkDraftState) {
    val saved = withContext(Dispatchers.IO) { runCatching { RustBridge.saveNetworkDraft(opened, draft) } }
    state.status = if (saved.isSuccess) DraftStatus.SAVED else DraftStatus.SAVE_FAILED
}

private fun startCheck(state: NetworkDraftState, scope: CoroutineScope, activateOnConnect: Boolean) {
    val checker = state.checker ?: return
    if (!state.editable) return
    val draft = networkSettings(state)
    val attempt = ConnectionAttempt(checker)
    state.attempt = attempt
    state.checkStatus = R.string.network_checking
    state.checkRows = emptyList()
    state.checkError = null
    // Adopt the already-created owner even if this scope was cancelled before IO dispatch.
    scope.launch(start = CoroutineStart.UNDISPATCHED) {
        try {
            checkDraft(attempt, draft, state)
            if (activateOnConnect && state.checkStatus == R.string.network_connected) {
                state.checkStatus = R.string.network_activating
                activateDraft(checker, state)
            }
        } finally {
            finishAttempt(state, attempt)
        }
    }
}

private suspend fun finishAttempt(state: NetworkDraftState, attempt: ConnectionAttempt) {
    if (state.attempt === attempt) state.attempt = null
    attempt.cancel()
    withContext(NonCancellable + Dispatchers.IO) { attempt.close() }
}

/** The form's buttons; the wizard activates as its own step, the section right after a good check. */
internal fun draftActions(
    opened: Opened.Ready,
    state: NetworkDraftState,
    scope: CoroutineScope,
    activateOnConnect: Boolean,
) = DraftActions(check = {
    startCheck(state, scope, activateOnConnect)
}, cancel = { state.attempt?.cancel() }, activate = {
    if (state.checkStatus == R.string.network_connected) {
        state.checker?.let { checker ->
            state.checkStatus = R.string.network_activating
            scope.launch { activateDraft(checker, state) }
        }
    }
}, save = {
    state.status = DraftStatus.SAVING
    scope.launch { saveDraft(opened, networkSettings(state), state) }
})

/** The seeds and name as the form shows them, without blank lines. */
internal fun networkSettings(state: NetworkDraftState) = NetworkSettings(
    state.seeds.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.toList(),
    state.name
)

/** The status line's text for this draft state. */
internal fun networkStatusText(status: DraftStatus): Int = when (status) {
    DraftStatus.LOADING -> R.string.network_loading
    DraftStatus.EDITING -> R.string.network_draft_only
    DraftStatus.SAVING -> R.string.network_saving
    DraftStatus.SAVED -> R.string.network_saved
    DraftStatus.READ_FAILED -> R.string.network_read_failed
    DraftStatus.SAVE_FAILED -> R.string.network_save_failed
}
