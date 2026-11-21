package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

internal val LocalActiveChatPeers = staticCompositionLocalOf<Set<String>> { emptySet() }

/** Optional marks fail closed; retaining chat rows must not retain stale presence. */
internal class ChatPresence {
    var peers by mutableStateOf(emptySet<String>())
        private set

    fun read(load: () -> List<String>) {
        peers = runCatching(load).getOrDefault(emptyList()).toSet()
    }
}

@Composable
internal fun ChatPresenceLabel(peer: String) {
    if (peer in LocalActiveChatPeers.current) {
        Caption(stringResource(R.string.chat_active_recently), Modifier.testTag("chat-presence-$peer"))
    }
}
