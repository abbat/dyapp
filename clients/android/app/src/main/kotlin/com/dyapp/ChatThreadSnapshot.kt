package com.dyapp

import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatItem
import uniffi.dyapp.ChatPhoto

/** Read under one bridge lease so text actions and mixed history describe the same identity. */
internal class ChatThreadSnapshot(
    val history: ChatHistory,
    val items: List<ChatItem>,
    val loadPhoto: ((ChatPhoto) -> ByteArray)? = null,
    val autoPhoto: (() -> Boolean)? = null,
    val sendPhoto: ((PreparedPhoto) -> Result<uniffi.dyapp.ChatSubmission>)? = null,
)

@androidx.compose.runtime.Composable
@Suppress("LongParameterList") // Snapshot, identity and existing stateless chat controls.
internal fun MediaChatContent(
    snapshot: ChatThreadSnapshot?,
    peer: String,
    me: String,
    form: ChatForm,
    actions: ChatActions
) {
    androidx.compose.runtime.CompositionLocalProvider(
        LocalChatItems provides snapshot?.items,
        LocalChatPhotoLoader provides snapshot?.loadPhoto,
        LocalChatPhotoAuto provides snapshot?.autoPhoto,
        LocalChatPhotoSender provides snapshot?.sendPhoto,
    ) {
        androidx.compose.runtime.key(me, peer) {
            ChatContent(peer, me, snapshot?.history, form, actions)
        }
    }
}

private const val MAX_PHOTO_ASPECT = 20f

internal fun photoAspect(photo: ChatPhoto): Float =
    (photo.width.coerceAtLeast(1u).toFloat() / photo.height.coerceAtLeast(1u).toFloat())
        .coerceIn(1f / MAX_PHOTO_ASPECT, MAX_PHOTO_ASPECT)

/** Full message identity: different authors or groups may reuse a logical ID. */
internal fun photoKey(photo: ChatPhoto): String =
    photo.groupId.joinToString("") { "%02x".format(it) } + ":" + photo.author + ":" +
        photo.id.joinToString("") { "%02x".format(it) }
