package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import kotlinx.coroutines.launch

/** Opening-bound presentation callbacks; none selects a receipt's possibly obsolete destination. */
internal class LinkReceiveActions(
    val start: () -> Unit,
    val cancel: () -> Unit,
    val dismiss: () -> Unit,
    val openApp: () -> Unit,
    val review: () -> Unit
)

internal class LinkReceiveNavigation(val back: () -> Unit, val openApp: () -> Unit, val review: () -> Unit)

private class LinkReceiveOwner(
    private val source: Opened.Ready,
    private val opening: LinkReceiverOpening?,
    private val store: LinkReceiverStore
) {
    private var present = true
    private val current get() = present && store.state.value === opening

    fun start() { if (current && opening == null) store.begin(source, false) }
    fun cancel() { if (current) opening?.session?.retire() }
    fun review(complete: () -> Unit) {
        if (current && opening?.session?.state?.value?.busy != true) complete()
    }
    suspend fun dispose(complete: () -> Unit) {
        val original = opening ?: return
        if (!current) return
        if (store.release(original) && present) complete()
    }
    fun retire() {
        present = false
        opening?.session?.retire()
    }
}

@Composable
internal fun rememberLinkReceiveActions(
    source: Opened.Ready,
    opening: LinkReceiverOpening?,
    navigation: LinkReceiveNavigation,
    store: LinkReceiverStore = pendingLinkReceiver
): LinkReceiveActions {
    val scope = rememberCoroutineScope()
    val owner = remember(source, opening, store) { LinkReceiveOwner(source, opening, store) }
    val route by rememberUpdatedState(navigation)
    DisposableEffect(owner) { onDispose(owner::retire) }
    return LinkReceiveActions(
        start = owner::start,
        cancel = owner::cancel,
        dismiss = { scope.launch { owner.dispose { route.back() } } },
        openApp = { scope.launch { owner.dispose { route.openApp() } } },
        review = { owner.review { route.review() } }
    )
}
