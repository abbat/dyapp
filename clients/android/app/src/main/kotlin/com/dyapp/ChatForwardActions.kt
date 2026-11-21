package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch

internal class ChatForwardActions(
    val context: ChatForwardContext,
    val shown: ForwardSelection,
    private val scope: CoroutineScope,
) {
    fun choose(target: String) = context.state.choose(shown, target)
    fun cancel() = context.state.cancel(shown)
    fun refresh() {
        scope.launch(start = CoroutineStart.UNDISPATCHED) { context.state.refresh(shown, context.calls) }
    }
    fun send() {
        scope.launch(start = CoroutineStart.UNDISPATCHED) { context.state.send(shown, context.calls) }
    }
}
