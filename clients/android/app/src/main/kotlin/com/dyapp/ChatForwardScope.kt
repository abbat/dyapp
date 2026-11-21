package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf

internal val LocalChatForward = staticCompositionLocalOf<ChatForwardContext?> { null }

@Composable
internal fun ChatForwardScope(
    feed: ChatFeed,
    peer: String,
    open: (String) -> Unit,
    content: @Composable () -> Unit,
) = key(feed.opened, peer) {
    val context = remember { nativeForwardContext(feed, peer) }
    DisposableEffect(context) { onDispose { context.state.retire() } }
    CompositionLocalProvider(LocalChatForward provides context, content = content)
    ChatForwardHost(context, open)
}

private fun nativeForwardContext(feed: ChatFeed, peer: String) = ChatForwardContext(
    feed.opened.peerId,
    peer,
    ChatForwardPresentation(),
    ChatForwardCalls(
        { source -> RustBridge.withChats(feed.opened) { it.forwardTargets(source) } },
        { source, target ->
            val result = RustBridge.withChats(feed.opened) { it.forwardText(source, target) }
            // A refresh failure cannot turn committed delivery into a retryable submission error.
            runCatching { feed.read() }
            result
        }
    )
)
