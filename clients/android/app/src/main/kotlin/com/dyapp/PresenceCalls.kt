package com.dyapp

/** The core owns audience, eligibility and throttling. */
internal fun presenceCalls(active: Opened.Ready) = PrivacyPreferenceCalls(
    read = { RustBridge.withChats(active) { it.presenceEnabled() } },
    write = { enabled -> RustBridge.withChats(active) { it.setPresenceEnabled(enabled) } },
)
