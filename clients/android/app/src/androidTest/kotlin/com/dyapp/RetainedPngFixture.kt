package com.dyapp

import android.content.Context
import uniffi.dyapp.ChatPhoto
import uniffi.dyapp.ChatSession
import uniffi.dyapp.PhotoPurpose
import java.io.File
import java.security.MessageDigest

/** Existing native registry/keystore state crosses invocations; only public proof enters JSON. */
internal class RetainedPngFixture(private val context: Context) {
    private val fixture = WorkerPeerFixture(
        context.noBackupFilesDir,
        File(context.filesDir, "png-peer-state.json")
    )
    private val proofFile = File(context.filesDir, "png-photo-proof.json")

    fun prepare() {
        workerTestPolicy()
        val owner = fixture.prepare()
        publishWorkerPeer(owner)
        val directory = RustBridge.withKey(owner) { path, _ -> File(path) }
        RustBridge.withChats(owner) { session ->
            HostPhotoPeer(owner.peerId).use { host ->
                val record = record(owner, host.peerId)
                record.begin()
                val photo = IncomingPngChecks(session, directory).run(host)
                record.recordPhoto(proof(photo, original(session, host.peerId, photo)))
            }
        }
    }

    fun reopenOffline() {
        val owner = fixture.reopen()
        val host = PeerCheckpoint(proofFile).read().getString("counterpart")
        val record = record(owner, host)
        record.requireFreshProcess()
        val expected = record.photo()
        val actual = RustBridge.withChats(owner) { session ->
            val photo = session.chatPhotos(host).single { photoKey(it) == expected.selector }
            check(photo.mime == "image/png") { "Retained original is not PNG" }
            proof(photo, original(session, host, photo))
        }
        check(actual == expected) { "Retained PNG selector, bytes or decoded geometry changed" }
    }

    fun cleanup() = fixture.cleanup()

    private fun record(owner: Opened.Ready, host: String) =
        WorkerMailRecord(proofFile, owner.peerId, host)

    private fun original(session: ChatSession, host: String, photo: ChatPhoto): ByteArray =
        session.chatPhotoOriginal(host, photo.groupId, photo.author, photo.id)

    private fun proof(photo: ChatPhoto, bytes: ByteArray): WorkerPhotoProof {
        val decoded = decodePhoto(bytes, PhotoPurpose.CHAT)
        return try {
            WorkerPhotoProof(photoKey(photo), digest(bytes), decoded.width, decoded.height)
        } finally {
            decoded.recycle()
        }
    }

    private fun digest(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
