package com.dyapp

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ContactNameView

/** Conversation rows within the Chats section's scroll container. */
@Composable
internal fun Conversations(rows: List<ChatRow>, names: Map<String, ContactNameView>, onOpen: (String) -> Unit) {
    Text(
        stringResource(R.string.chats_conversations),
        Modifier.padding(16.dp),
        style = MaterialTheme.typography.titleMedium
    )
    if (rows.isEmpty()) {
        Text(stringResource(R.string.chats_empty), Modifier.padding(horizontal = 16.dp))
    } else {
        // Not lazy, as it scrolls with the lists above; all rows are in memory anyway.
        Column(Modifier.testTag("chat-list")) {
            for ((index, row) in rows.withIndex()) {
                ChatListRow(
                    row,
                    names[row.peerId],
                    Modifier.testTag("chat-${index + 1}").clickable { onOpen(row.peerId) }
                )
            }
        }
    }
}
