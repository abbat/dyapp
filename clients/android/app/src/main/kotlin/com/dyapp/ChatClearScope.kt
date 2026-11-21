package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf

internal val LocalChatClearCalls = staticCompositionLocalOf<ChatClearCalls?> { null }

@Composable
internal fun ChatClearScope(opened: Opened.Ready, peer: String, content: @Composable () -> Unit) {
    val calls = remember(opened, peer) {
        ChatClearCalls { RustBridge.withChats(opened) { it.clearTarget(peer) } }
    }
    CompositionLocalProvider(LocalChatClearCalls provides calls, content = content)
}
