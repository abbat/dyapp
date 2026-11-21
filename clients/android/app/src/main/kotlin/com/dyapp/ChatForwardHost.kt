package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope

@Composable
internal fun ChatForwardHost(context: ChatForwardContext, open: (String) -> Unit) {
    val state = context.state
    state.opening?.let { shown ->
        key(shown) {
            val scope = rememberCoroutineScope()
            val actions = remember { ChatForwardActions(context, shown, scope) }
            LaunchedEffect(shown) { state.refresh(shown, context.calls) }
            ChatForwardDialog(state, actions)
        }
    }
    val receipt = state.submitted
    LaunchedEffect(receipt) { receipt?.let { state.complete(it, open) } }
}
