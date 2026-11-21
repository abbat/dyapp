package com.dyapp

internal class ChatForwardContext(
    val ownerPeerId: String,
    val peer: String,
    val state: ChatForwardPresentation,
    val calls: ChatForwardCalls,
)
