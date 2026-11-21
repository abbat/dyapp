package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Presentation admission only: failed/unknown values never authorize a guessed preference. */
internal class PrivacyPreferenceState {
    var value by mutableStateOf<Boolean?>(null)
        private set
    var busy by mutableStateOf(true)
        private set
    var failed by mutableStateOf(false)
        private set
    var revision by mutableStateOf(Any())
        private set
    private var active = true

    fun editable(): Boolean {
        if (!active) return false
        return value != null && !busy && !failed
    }

    fun begin(target: Boolean?, shown: Any): Boolean {
        val current = active && shown === revision && !busy
        val permitted = target == null || editable() && value != target
        if (!current || !permitted) return false
        busy = true
        failed = false
        return true
    }

    fun complete(result: Result<Boolean>) {
        if (!active) return
        failed = result.isFailure
        result.onSuccess { value = it }
        busy = false
        revision = Any()
    }

    fun retire() { active = false }
}
