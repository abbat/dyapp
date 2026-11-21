package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import kotlinx.coroutines.launch
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.MobileSlotSelection

/** Opening-bound presentation callbacks; none selects a receipt's possibly obsolete destination. */
internal class LinkReceiveActions(
    val start: () -> Unit,
    val cancel: () -> Unit,
    val dismiss: () -> Unit,
    val openApp: () -> Unit,
    val review: () -> Unit,
    val enrollment: LinkEnrollmentActions
)

internal class LinkReceiveNavigation(val back: () -> Unit, val openApp: () -> Unit, val review: () -> Unit)

private class LinkReceiveOwner(
    private val source: LinkReceiveSource,
    private val opening: LinkReceiverOpening?,
    private val store: LinkReceiverStore
) {
    private var present = true
    private val current get() = present && store.state.value === opening

    fun start() { if (current && opening == null) store.begin(source.opened, source.wizard) }
    fun cancel() { if (current) opening?.session?.retire() }
    fun review(complete: () -> Unit) {
        if (current && opening?.session?.state?.value?.busy != true) complete()
    }
    suspend fun dispose(complete: () -> Unit) {
        val original = opening ?: return
        if (!current) return
        if (store.release(original) && present) complete()
    }
    fun finish() {
        val original = opening ?: return
        if (!current) return
        original.session.finishEnrollment(::nativeReview)
    }
    fun prepare() {
        val original = opening ?: return
        if (current) original.session.prepareEnrollmentReview(::nativeReview)
    }
    fun confirm(expected: LinkReviewOpening, choices: List<LinkReviewChoice>) =
        if (current) opening?.session?.confirmEnrollmentReview(expected, choices) else null
    private fun nativeReview(selection: MobileSlotSelection): LinkEnrollmentReview {
        val original = checkNotNull(opening)
        return linkEnrollmentReview(original.source.slot.owner(), selection.slot.id, selection.key.peerId())
    }
    fun retire() {
        present = false
        opening?.session?.retire()
    }
}

@Composable
internal fun rememberLinkReceiveActions(
    source: LinkReceiveSource,
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
        review = { owner.review { route.review() } },
        enrollment = LinkEnrollmentActions(owner::finish, owner::prepare, owner::confirm)
    )
}
