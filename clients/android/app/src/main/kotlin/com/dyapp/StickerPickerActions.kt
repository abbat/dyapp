package com.dyapp

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** Call admission runs on the event turn; native IO remains owned by the presentation operation. */
internal class StickerPickerActions(
    private val state: StickerPickerPresentation,
    private val calls: StickerPickerCalls,
    private val scope: CoroutineScope,
    private val closeView: () -> Unit,
    private val manageView: () -> Unit
) {
    fun refresh() = run { state.refresh(calls) }
    fun navigate(query: StickerPickerQuery, shown: StickerPickerView) = run { state.navigate(calls, query, shown) }
    fun back(shown: StickerPickerView) = run { state.back(calls, shown) }
    fun send(shown: StickerPickerView, choice: StickerPickerChoice) = run { state.send(calls, shown, choice) }

    fun close() {
        state.retire()
        closeView()
    }

    fun manage() {
        state.retire()
        manageView()
    }

    private fun run(action: suspend () -> Unit) {
        if (scope.isActive) scope.launch(start = CoroutineStart.UNDISPATCHED) { action() }
    }
}
