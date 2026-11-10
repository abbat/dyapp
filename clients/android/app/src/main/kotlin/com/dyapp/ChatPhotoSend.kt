package com.dyapp

/** Upload refusals retain the draft; only the native outcome establishes commitment. */
internal fun photoSendStatus(result: Result<uniffi.dyapp.ChatSubmission>): ChatStatus? = result.fold(
    onSuccess = { if (it is uniffi.dyapp.ChatSubmission.Queued) ChatStatus(R.string.chat_queued, null) else null },
    onFailure = {
        val details = it.toString()
        val text = when {
            "full" in details.lowercase() -> R.string.chat_photo_full
            "tooLarge" in details -> R.string.profile_photo_too_large
            else -> R.string.chat_send_failed
        }
        ChatStatus(text, details)
    },
)
