package com.dyapp

import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

/** Deliberately contradict the typed preview so a legacy text/time fallback cannot pass. */
internal fun previewRow(preview: ChatListPreview) = ChatRow(
    peerId = "conversation",
    state = ChatState.OPEN,
    lastMessage = message("conversation", "OLDER_UNBOUNDED_LAST_MESSAGE").copy(sentAtMs = 1uL),
    preview = preview,
    unreadCount = 2uL,
    muted = true
)
