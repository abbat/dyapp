package com.dyapp

/** Upload refusals never stage a message; delivery failures can leave one queued. */
internal fun photoSendStatus(error: Throwable, staged: Boolean): ChatStatus {
    if (staged) return ChatStatus(R.string.chat_queued, null)
    val details = error.toString()
    val text = when {
        "full" in details.lowercase() -> R.string.chat_photo_full
        "tooLarge" in details -> R.string.profile_photo_too_large
        else -> R.string.chat_send_failed
    }
    return ChatStatus(text, details)
}
