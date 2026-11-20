package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** UI admission occurs before dispatch; native authorization and proof remain in the core. */
internal fun launchPublication(scope: CoroutineScope, state: PublishState, publish: suspend () -> Unit) {
    if (!scope.isActive || !state.open || state.running) return
    if (!state.adult || !state.public) return
    scope.launch(start = CoroutineStart.UNDISPATCHED) {
        state.running = true
        try {
            publish()
        } finally {
            state.running = false
        }
    }
}
