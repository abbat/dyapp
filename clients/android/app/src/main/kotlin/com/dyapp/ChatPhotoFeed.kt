package com.dyapp

/** Blocking photo transport through the identity's shared chat lease; call on IO. */
internal class ChatPhotoFeed(private val opened: Opened.Ready, private val changed: () -> Unit) {
    fun original(peer: String, photo: uniffi.dyapp.ChatPhoto) = RustBridge.withChats(opened) {
        it.chatPhotoOriginal(peer, photo.groupId, photo.author, photo.id)
    }

    fun auto(peer: String) = RustBridge.withChats(opened) { it.chatPhotoAuto(peer) }

    /** Preserve staged-send handling; dimensions come from the core's prepared image. */
    fun send(peer: String, photo: PreparedPhoto): ChatStatus? {
        val status = RustBridge.withChats(opened) { chats ->
            val before = chats.chatPhotos(peer).map(::photoKey).toSet()
            val error = runCatching {
                chats.sendChatPhoto(peer, photo.full, photo.thumbnail, photo.width, photo.height)
            }.exceptionOrNull() ?: return@withChats null
            val staged = chats.chatPhotos(peer).any { it.author == opened.peerId && photoKey(it) !in before }
            photoSendStatus(error, staged)
        }
        changed()
        return status
    }
}
