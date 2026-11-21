package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.referentialEqualityPolicy
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import uniffi.dyapp.StickerPackSaveAttempt
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveStatus

/** Native calls retain the selected message and actual opened owner's operation lease. */
internal class StickerPackCalls(
    val read: suspend () -> StickerPackSaveStatus,
    val change: suspend (StickerPackSaveStatus, StickerPackSaveControl) -> StickerPackSaveAttempt,
)

/** Only presentation admission and recovery; the core supplies controls and authorizes mutation. */
internal class StickerPackPresentation {
    var status by mutableStateOf<StickerPackSaveStatus?>(null, referentialEqualityPolicy())
        private set
    var busy by mutableStateOf(false)
        private set
    var readFailed by mutableStateOf(false)
        private set
    var actionFailed by mutableStateOf(false)
        private set
    private var active = true
    private var known = false

    fun retire() { active = false }

    fun allows(expected: StickerPackSaveStatus, control: StickerPackSaveControl): Boolean =
        active && !busy && known && status === expected && control in expected.controls

    suspend fun refresh(calls: StickerPackCalls) {
        if (!active || busy) return
        busy = true
        actionFailed = false
        try {
            accept(read(calls))
        } finally { busy = false }
    }

    suspend fun change(
        calls: StickerPackCalls,
        expected: StickerPackSaveStatus,
        control: StickerPackSaveControl
    ) {
        if (!allows(expected, control)) return
        busy = true
        actionFailed = false
        try {
            val attempt = withContext(Dispatchers.IO) { runCatching { calls.change(expected, control) } }
            currentCoroutineContext().ensureActive()
            if (!active) return
            val ready = attempt.getOrNull() as? StickerPackSaveAttempt.Ready
            if (ready != null) {
                accept(Result.success(ready.status))
            } else {
                actionFailed = true
                // Application may have completed: reconcile by reading, never resubmit.
                accept(read(calls))
            }
        } finally { busy = false }
    }

    private suspend fun read(calls: StickerPackCalls): Result<StickerPackSaveStatus> =
        withContext(Dispatchers.IO) { runCatching { calls.read() } }

    private suspend fun accept(result: Result<StickerPackSaveStatus>) {
        currentCoroutineContext().ensureActive()
        if (!active) return
        known = result.isSuccess
        readFailed = result.isFailure
        result.onSuccess { status = it }
    }
}
