package com.dyapp

import uniffi.dyapp.ChatForwardSource
import uniffi.dyapp.ChatSubmission

/** Native policy/commitment only; the picker never supplies forwarded content. */
internal class ChatForwardCalls(
    val read: (ChatForwardSource) -> List<String>,
    val send: (ChatForwardSource, String) -> ChatSubmission,
)
