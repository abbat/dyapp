package com.dyapp

import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.ChatSubmission
import uniffi.dyapp.PhotoPurpose
import uniffi.dyapp.preparePhotoPixels
import java.security.MessageDigest

/** Reuses the host-owned offline/reboot/upgrade phases; Android remains a native client. */
internal class WorkerPhotoCache(
    private val owner: Opened.Ready,
    private val sender: Opened.Ready,
    private val record: WorkerMailRecord,
) {
    fun prepare() {
        val prepared = preparePhotoPixels(byteArrayOf(64, 32, 16, -1), 1u, 1u, PhotoPurpose.CHAT)
        val submitted = RustBridge.withChats(sender) {
            it.submitChatPhoto(owner.peerId, prepared.full, prepared.thumbnail, prepared.width, prepared.height)
        }
        check(submitted is ChatSubmission.Accepted) { "Online worker photo was not accepted" }
        val photo = receive(submitted.id)
        val bytes = original(photo)
        check(bytes.contentEquals(prepared.full)) { "Worker received a different original" }
        record.recordPhoto(
            WorkerPhotoProof(photoKey(photo), digest(bytes), prepared.width.toInt(), prepared.height.toInt())
        )
        verify()
    }

    fun verify() {
        val proof = record.photo()
        val photo = RustBridge.withChats(owner) { it.chatPhotos(sender.peerId) }
            .single { photoKey(it) == proof.selector }
        val bytes = original(photo)
        check(digest(bytes) == proof.sha256) { "Retained worker original bytes changed" }
        val decoded = decodePhoto(bytes, PhotoPurpose.CHAT)
        try {
            check(decoded.width == proof.width && decoded.height == proof.height) {
                "Retained worker original dimensions changed"
            }
        } finally {
            decoded.recycle()
        }
    }

    private fun receive(id: ByteArray): ChatPhoto {
        var found: ChatPhoto? = null
        PeerTransition("worker incoming photo").await(
            ready = {
                found = RustBridge.withChats(owner) { it.chatPhotos(sender.peerId) }
                    .singleOrNull { it.author == sender.peerId && it.id.contentEquals(id) }
                found != null
            },
            // Preparation alone receives directly; scheduled catch-up keeps its existing worker proof.
            advance = { RustBridge.withChats(owner) { it.receive() } }
        )
        return checkNotNull(found)
    }

    private fun original(photo: ChatPhoto): ByteArray = RustBridge.withChats(owner) {
        it.chatPhotoOriginal(sender.peerId, photo.groupId, photo.author, photo.id)
    }

    private fun digest(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
