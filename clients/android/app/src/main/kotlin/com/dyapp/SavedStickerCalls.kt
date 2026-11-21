package com.dyapp

import uniffi.dyapp.SavedStickerPackImport
import uniffi.dyapp.SavedStickerPackPage

/** Core owns collection pages, archive validation and removal; callers dispatch blocking work on IO. */
internal class SavedStickerCalls(
    val page: suspend (ByteArray?) -> SavedStickerPackPage,
    val save: suspend (ByteArray) -> SavedStickerPackImport,
    val remove: suspend (ByteArray) -> Boolean,
)

internal fun savedStickerCalls(opened: Opened.Ready) = SavedStickerCalls(
    page = { after -> RustBridge.withChats(opened) { it.savedStickerPacks(after) } },
    save = { bytes -> RustBridge.withChats(opened) { it.importStickerPack(bytes) } },
    remove = { id ->
        opened.slot.owner().operation(opened.slot.id, opened.peerId).use { it.removeSavedStickerPack(id) }
    }
)
