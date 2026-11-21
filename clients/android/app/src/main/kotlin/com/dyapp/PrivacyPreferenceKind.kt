package com.dyapp

/** UI labels only; native Store supplies values and owns the disclosure rules. */
internal enum class PrivacyPreferenceKind(
    val title: Int,
    val hint: Int?,
    val tag: String,
    val failure: Int,
    val statusTag: String
) {
    VIEWS(
        R.string.profile_views_toggle,
        R.string.profile_views_hint,
        "send-profile-views",
        R.string.privacy_preference_failed,
        "profile-views"
    ),
    RECEIPTS(
        R.string.receipts_toggle,
        R.string.receipts_hint,
        "send-read-receipts",
        R.string.receipts_failed,
        "receipts"
    ),
    TYPING(
        R.string.typing_toggle,
        null,
        "send-typing",
        R.string.privacy_preference_failed,
        "typing"
    )
}

internal fun privacyPreferenceCalls(active: Opened.Ready, kind: PrivacyPreferenceKind): PrivacyPreferenceCalls =
    when (kind) {
        PrivacyPreferenceKind.VIEWS -> PrivacyPreferenceCalls(
            read = { RustBridge.withChats(active) { it.profileViewsEnabled() } },
            write = { value -> RustBridge.withChats(active) { it.setProfileViewsEnabled(value) } }
        )
        PrivacyPreferenceKind.RECEIPTS -> PrivacyPreferenceCalls(
            read = { RustBridge.withChats(active) { it.readReceipts() } },
            write = { value -> RustBridge.withChats(active) { it.setReadReceipts(value) } }
        )
        PrivacyPreferenceKind.TYPING -> PrivacyPreferenceCalls(
            read = { RustBridge.withChats(active) { it.typingEnabled() } },
            write = { value -> RustBridge.withChats(active) { it.setTypingEnabled(value) } }
        )
    }
