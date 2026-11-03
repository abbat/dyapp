package com.dyapp

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatSession
import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.ProfileSolve
import uniffi.dyapp.PublicProfileDraft
import uniffi.dyapp.newChatStateKey
import uniffi.dyapp.saveProfileDraft
import uniffi.dyapp.setTestNodeIdPowBits
import java.io.File

internal const val PEER_WAIT_MS = 180_000L
private const val POLL_MS = 2_000L
private const val CHECK_TRIES = 3

/** Polls [done] until it holds; a timeout names [what]. */
internal fun waitFor(what: String, timeoutMs: Long = PEER_WAIT_MS, done: () -> Boolean) {
    val end = SystemClock.uptimeMillis() + timeoutMs
    while (!done()) {
        check(SystemClock.uptimeMillis() < end) { "Timed out waiting for $what" }
        SystemClock.sleep(POLL_MS)
    }
}

/**
 * A second identity that the test drives through the FFI, as the other phone would: its own
 * folder outside the identity slots, the local node as its network and a published profile.
 */
internal class TestPeer(name: String) : AutoCloseable {
    // The cache, not noBackupFilesDir: the app would list a folder there as a broken identity.
    private val dir = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "peer-$name")
        .apply { deleteRecursively(); mkdirs() }
    private val path = dir.absolutePath
    private val alias = "dyapp-test-peer:$name"
    private val key = IdentityKey.generate()
    private val device = IdentityKey.generate()
    val peerId: String = key.peerId()
    val session: ChatSession

    init {
        setTestNodeIdPowBits(8u)
        saveProfileDraft(path, key, PublicProfileDraft(
            gender = 1u, orientation = 0u, relationshipGoal = 0u, rolePreference = 0u, careerAmbition = 0u,
            financialPhilosophy = 0u, employmentStatus = 0u, fitnessLevel = 0u, zodiac = 0u,
            birthYear = 1990u, birthMonth = 1u, birthDay = 1u, country = "", place = "",
        ))
        activateLocalNode()
        ProfileSolve(path, key).use { solve -> generateSequence { solve.runStep(true) }.first { it.solved } }
        val store = PlatformKeyStore(alias)
        session = ChatSession.open(path, key, device, store, newChatStateKey(store))
        session.publishProfile(true)
    }

    // A cold emulator's first dial can outlast one check, as in the UI helper.
    private fun activateLocalNode() = ConnectionCheck(path).use { checker ->
        val settings = NetworkSettings(listOf(LOCAL_NODE), "local")
        val activated = (1..CHECK_TRIES).any {
            checker.run(settings, CHECK_WAIT_MS.toUInt())
            runCatching { checker.activate() }.isSuccess
        }
        check(activated) { "The test peer could not activate $LOCAL_NODE" }
    }

    /** Receives until a message from [author] in their chat satisfies [match]. */
    fun awaitMessage(author: String, what: String, match: (ChatMessage) -> Boolean) {
        waitFor(what) {
            runCatching { session.receive() }
            runCatching { session.chatHistory(author).messages }.getOrDefault(emptyList())
                .any { it.author == author && match(it) }
        }
    }

    override fun close() {
        session.close()
        key.close()
        device.close()
        PlatformKeyStore.delete(alias)
        dir.deleteRecursively()
    }
}
