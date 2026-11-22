package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** One presentation admission for setup actions; native mode/authorization remain authoritative. */
internal class AppLockSetupSubmission(private val scope: CoroutineScope) {
    var busy by mutableStateOf(false)
        private set
    var error by mutableStateOf<Int?>(null)

    fun start(failure: Int, action: suspend () -> Boolean) {
        if (busy) return
        busy = true
        error = null
        scope.launch {
            try {
                if (!action()) error = failure
            } finally {
                busy = false
            }
        }
    }
}
