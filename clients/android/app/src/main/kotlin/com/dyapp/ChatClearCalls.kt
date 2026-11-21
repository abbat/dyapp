package com.dyapp

import uniffi.dyapp.ChatClearTarget

/** Read-only native admission, bound to one actual identity opening and peer. */
internal class ChatClearCalls(val read: () -> ChatClearTarget)
