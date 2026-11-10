package com.dyapp

/** Blocking photo transport through the identity's shared chat lease; call on IO. */
internal class ChatPhotoFeed(private val opened: Opened.Ready, private val changed: () -> Unit) {
    fun original(peer: String, photo: uniffi.dyapp.ChatPhoto) = RustBridge.withChats(opened) {
        it.chatPhotoOriginal(peer, photo.groupId, photo.author, photo.id)
    }

    fun auto(peer: String) = RustBridge.withChats(opened) { it.chatPhotoAuto(peer) }

    /** The core distinguishes upload refusal from a committed photo awaiting delivery. */
    fun send(peer: String, photo: PreparedPhoto) = runCatching {
        RustBridge.withChats(opened) {
            it.submitChatPhoto(peer, photo.full, photo.thumbnail, photo.width, photo.height)
        }
    }.also { changed() }
}
