package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/** Exists only in the unlocked identity shell; mailbox passes never invoke it. */
@Composable
internal fun ChatPresenceForeground(opened: Opened.Ready) {
    val owner = checkNotNull(LocalView.current.findViewTreeLifecycleOwner()) { "Compose needs a lifecycle" }
    ChatForegroundEffect(owner, opened) {
        withContext(Dispatchers.IO) {
            val admission = currentCoroutineContext()
            runCatching {
                RustBridge.withChats(opened) {
                    // Native admission can wait behind other work; a retired event must not send.
                    admission.ensureActive()
                    it.announcePresence()
                }
            }
        }
    }
}

/** One attempt per actual resume or identity opening, without retry timers or recomposition sends. */
@Composable
internal fun ChatForegroundEffect(owner: LifecycleOwner, identity: Any, announce: suspend () -> Unit) {
    val currentAnnounce by rememberUpdatedState(announce)
    LaunchedEffect(owner, identity) {
        owner.repeatOnLifecycle(Lifecycle.State.RESUMED) { currentAnnounce() }
    }
}
