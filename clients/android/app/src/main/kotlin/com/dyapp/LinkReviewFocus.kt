package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalWindowInfo

internal class LinkReviewFocus {
    val preview = FocusRequester()
    val edit = FocusRequester()
    val back = FocusRequester()
    fun request(previewing: Boolean) = if (previewing) edit.requestFocus() else preview.requestFocus()
}

/** Focus follows composition, enabled state and window readiness; each transition is admitted once. */
@Composable
internal fun rememberLinkReviewFocus(
    previewing: Boolean,
    enabled: Boolean,
    failure: Throwable? = null
): LinkReviewFocus {
    val focus = remember { LinkReviewFocus() }
    var viewed by remember { mutableStateOf(false) }
    var pending by remember { mutableStateOf<Boolean?>(null) }
    val ready = enabled && LocalWindowInfo.current.isWindowFocused
    LaunchedEffect(previewing) {
        if (previewing) {
            viewed = true
            pending = true
        } else if (viewed) {
            pending = false
        }
    }
    LaunchedEffect(pending, ready) {
        val target = pending
        if (target == null || target != previewing || !ready) return@LaunchedEffect
        if (focus.request(target)) pending = null
    }
    LinkReviewFailureFocus(focus.back, failure, ready)
    return focus
}

@Composable
private fun LinkReviewFailureFocus(requester: FocusRequester, failure: Throwable?, ready: Boolean) {
    var pending by remember { mutableStateOf(false) }
    LaunchedEffect(failure) { pending = failure != null }
    LaunchedEffect(pending, ready) {
        if (pending && ready && requester.requestFocus()) pending = false
    }
}
