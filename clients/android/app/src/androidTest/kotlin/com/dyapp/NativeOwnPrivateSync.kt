package com.dyapp

/** After initial link import, prove later changes through the normal native receive path. */
internal fun verifyNativeOwnPrivateSync(first: Opened.Ready, second: Opened.Ready, contact: String) {
    var joined = false
    // These isolated peers have no contact chats or other mail. A receive change therefore observes
    // the own-group Welcome; the initiator already committed that group before sending it.
    PeerTransition("native own-group Welcome completion").await(
        ready = { joined },
        advance = {
            listOf(first, second).forEach { owner ->
                val changed = RustBridge.withChats(owner) { it.receive() }
                joined = changed > 0u || joined
            }
        }
    )
    exchangePrivateContact(first to second, contact, "Later private note from the original device", saved = false)
    exchangePrivateContact(second to first, contact, "Later private note from the linked device", saved = true)
}

private fun exchangePrivateContact(
    owners: Pair<Opened.Ready, Opened.Ready>,
    contact: String,
    note: String,
    saved: Boolean,
) {
    RustBridge.withChats(owners.first) {
        it.setFavorite(contact, saved)
        it.setContactNote(contact, note)
    }
    PeerTransition("incremental private state from ${owners.first.slot.id}").await(
        ready = {
            RustBridge.withChats(owners.second) {
                it.favoriteState(contact) == saved && it.contactNote(contact) == note
            }
        },
        advance = {
            listOf(owners.first, owners.second).forEach { owner ->
                RustBridge.withChats(owner) { it.receive() }
            }
        }
    )
    assertNativePrivateContact(owners.second, contact, note, saved)
}
