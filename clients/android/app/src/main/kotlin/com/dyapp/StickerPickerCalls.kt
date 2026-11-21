package com.dyapp

import uniffi.dyapp.ChatSession
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.SavedStickerPackPage
import uniffi.dyapp.StickerPickerEntry

/** Navigation transports core cursors verbatim; it never computes a manifest/recent position. */
internal sealed interface StickerPickerQuery {
    class Packs(val after: ByteArray? = null) : StickerPickerQuery
    class Choices(val packId: ByteArray? = null, val after: UInt = 0u) : StickerPickerQuery
}

/** Each displayed choice is also its callback identity; content-equal replacements retire old buttons. */
internal class StickerPickerChoice(val packId: ByteArray, val name: String, val sticker: StickerPickerEntry)

internal sealed interface StickerPickerData {
    class Packs(val page: SavedStickerPackPage) : StickerPickerData
    class Choices(val entries: List<StickerPickerChoice>, val next: UInt?) : StickerPickerData
}

internal class StickerPickerCalls(
    val read: suspend (StickerPickerQuery) -> StickerPickerData,
    val send: suspend (StickerPickerChoice) -> ChatSubmission
)

/** Native operation ownership outlives a cancelled view; no text draft is passed to this adapter. */
internal fun stickerPickerCalls(opened: Opened.Ready, peer: String, changed: () -> Unit = {}) = StickerPickerCalls(
    read = { query -> RustBridge.withChats(opened) { readStickerPicker(it, query) } },
    send = { choice ->
        RustBridge.withChats(opened) { it.submitSavedSticker(peer, choice.packId, choice.sticker.index) }
            .also { runCatching(changed) } // Refresh failure cannot turn a committed send into a retry.
    }
)

private fun readStickerPicker(session: ChatSession, query: StickerPickerQuery): StickerPickerData = when (query) {
    is StickerPickerQuery.Packs -> StickerPickerData.Packs(session.savedStickerPacks(query.after))
    is StickerPickerQuery.Choices -> query.packId?.let { id ->
        val page = session.savedStickerPage(id, query.after)
        StickerPickerData.Choices(
            page.entries.map { StickerPickerChoice(page.packId, page.name, it) }, page.next
        )
    } ?: session.recentStickerPage(query.after).let { page ->
        StickerPickerData.Choices(
            page.entries.map { StickerPickerChoice(it.packId, it.packName, it.sticker) }, page.next
        )
    }
}
