package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ContactNameView

internal fun ChatRow.sharedProfilePeer(): String? = (preview.body as? ChatPreviewBody.Profile)?.peerId

/** Core-selected content is already bounded; formatting never fetches a linked profile or media. */
@Composable
internal fun chatListPreview(preview: ChatListPreview, sharedName: ContactNameView?): String {
    val text = when (val body = preview.body) {
        ChatPreviewBody.Empty -> stringResource(R.string.chats_no_messages)
        ChatPreviewBody.Deleted -> stringResource(R.string.chats_deleted)
        is ChatPreviewBody.Text -> body.text
        ChatPreviewBody.Photo -> stringResource(R.string.chat_photo)
        is ChatPreviewBody.Sticker -> stringResource(R.string.chat_sticker_fallback, body.emoji, body.packName)
        is ChatPreviewBody.Profile -> stringResource(
            R.string.shared_profile_card,
            contactLabel(body.peerId, sharedName)
        )
    }
    return if (preview.forwarded) stringResource(R.string.chats_forwarded, text) else text
}

/** Metadata has its own lines, so even long localized times cannot consume preview width. */
@Composable
internal fun ChatListMetadata(row: ChatRow) {
    Column {
        chatTime(row.preview.sentAtMs)?.let {
            Text(it, Modifier.testTag("chat-preview-time"), style = MaterialTheme.typography.labelSmall)
        }
        if (row.unreadCount > 0uL) UnreadBadge(row.unreadCount, Modifier)
        if (row.muted) Caption(stringResource(R.string.chats_muted), Modifier.testTag("chat-muted"))
    }
}
