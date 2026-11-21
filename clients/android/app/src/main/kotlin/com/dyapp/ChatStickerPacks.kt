package com.dyapp

import uniffi.dyapp.ChatSticker
import uniffi.dyapp.StickerPackSaveAction
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveStatus

/** Presentation hides own-source controls; native full-message selection still authorizes every call. */
internal fun receivedStickerPackTarget(
    opened: Opened.Ready,
    peer: String,
    sticker: ChatSticker,
    revision: Int
): StickerPackTarget? {
    if (sticker.author == opened.peerId) return null
    val calls = StickerPackCalls(
        read = {
            RustBridge.withChats(opened) {
                it.chatStickerPackStatus(peer, sticker.groupId, sticker.author, sticker.id)
            }
        },
        change = { shown, control ->
            val action = stickerPackAction(shown, control)
            RustBridge.withChats(opened) {
                it.changeStickerPackRequest(peer, sticker.groupId, sticker.author, sticker.id, action)
            }
        }
    )
    return StickerPackTarget(listOf(opened, peer, stickerKey(sticker)), sticker.packName, calls, revision)
}

/** No clocks, eligibility table or generated token: use the exact core projection shown to the user. */
internal fun stickerPackAction(shown: StickerPackSaveStatus, control: StickerPackSaveControl): StickerPackSaveAction =
    when (control) {
        StickerPackSaveControl.REQUEST -> StickerPackSaveAction.Request
        StickerPackSaveControl.RETRY -> StickerPackSaveAction.Retry(checkNotNull(shown.generation))
        StickerPackSaveControl.FRESH -> StickerPackSaveAction.Fresh(checkNotNull(shown.generation))
        StickerPackSaveControl.STOP_WAITING -> StickerPackSaveAction.StopWaiting(checkNotNull(shown.generation))
        StickerPackSaveControl.REFRESH, StickerPackSaveControl.MANAGE -> error("Read-only pack control")
    }
