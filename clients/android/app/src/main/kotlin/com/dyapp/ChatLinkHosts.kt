package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.chatLinkHosts

/** Display-only destinations; parsing, credential removal and IDN normalization stay in the core. */
@Composable
internal fun ChatLinkHosts(message: ChatMessage) {
    val text = message.text?.takeUnless { message.deleted } ?: return
    val hosts = remember(text) { chatLinkHosts(text) }
    if (hosts.isEmpty()) return
    Caption(
        stringResource(R.string.chat_link_hosts, hosts.joinToString(", ") { it.ascii }),
        Modifier.testTag("chat-link-hosts")
    )
}
