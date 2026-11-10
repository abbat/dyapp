package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.ProfileSolve
import uniffi.dyapp.PublicProfileDraft
import uniffi.dyapp.SeedRow
import uniffi.dyapp.addProfilePhoto
import uniffi.dyapp.newChatStateKey
import uniffi.dyapp.saveProfileDraft
import uniffi.dyapp.setTestNodeIdPowBits
import uniffi.dyapp.setTestProfilePowBits
import java.io.ByteArrayOutputStream
import java.io.File

internal const val PEER_WAIT_MS = 180_000L
private const val CHECK_TRIES = 3

private fun redPng(): ByteArray {
    val bitmap = createBitmap(640, 480, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
    return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
}

/**
 * A second identity that the test drives through the FFI, as the other phone would: its own
 * folder outside the identity slots, the local node as its network and a published profile.
 */
internal class TestPeer(name: String, photo: Boolean = false) : AutoCloseable {
    // The cache, not noBackupFilesDir: the app would list a folder there as a broken identity.
    private val dir = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "peer-$name")
        .apply {
            deleteRecursively()
            mkdirs()
        }
    private val path = dir.absolutePath
    private val alias = "dyapp-test-peer:$name"
    private val key = IdentityKey.generate()
    private val device = IdentityKey.generate()
    val peerId: String = key.peerId()
    val session: ChatSession

    init {
        setTestNodeIdPowBits(8u)
        setTestProfilePowBits(4u)
        saveProfileDraft(
            path,
            key,
            PublicProfileDraft(
                gender = 1u, orientation = 0u, relationshipGoal = 0u, rolePreference = 0u, careerAmbition = 0u,
                financialPhilosophy = 0u, employmentStatus = 0u, fitnessLevel = 0u, zodiac = 0u,
                birthYear = 1990u, birthMonth = 1u, birthDay = 1u, country = "", place = "",
            )
        )
        activateLocalNode()
        ProfileSolve(path, key).use { solve -> generateSequence { solve.runStep(true) }.first { it.solved } }
        if (photo) {
            val prepared = preparePhoto(redPng())
            addProfilePhoto(path, key, prepared.full, prepared.thumbnail)
        }
        val store = PlatformKeyStore(alias)
        session = ChatSession.open(path, key, device, store, newChatStateKey(store))
        session.publishProfile(true)
    }

    // A cold emulator's first dial can outlast one check, as in the UI helper.
    private fun activateLocalNode() = ConnectionCheck(path).use { checker ->
        // The local node runs the public network: an empty name, as in NetworkDraftTests.
        val settings = NetworkSettings(listOf(LOCAL_NODE), "")
        var rows = emptyList<SeedRow>()
        val activated = (1..CHECK_TRIES).any {
            rows = checker.run(settings, CHECK_WAIT_MS.toUInt())
            runCatching { checker.activate() }.isSuccess
        }
        check(activated) { "The test peer could not activate $LOCAL_NODE: $rows" }
    }

    /** Receives until a message from [author] in their chat satisfies [match]. */
    fun awaitMessage(author: String, what: String, match: (ChatMessage) -> Boolean) {
        PeerTransition(what).await(
            ready = { session.chatHistory(author).messages.any { it.author == author && match(it) } },
            // receive returns only after mailbox application; its completion drives the next read.
            advance = { session.receive() }
        )
    }

    override fun close() {
        session.close()
        key.close()
        device.close()
        PlatformKeyStore.delete(alias)
        dir.deleteRecursively()
    }
}
