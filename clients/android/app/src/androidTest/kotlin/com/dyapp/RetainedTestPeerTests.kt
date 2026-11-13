package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.UUID

class RetainedTestPeerTests {
    private fun root() = File(
        InstrumentationRegistry.getInstrumentation().targetContext.filesDir,
        "retained-peer-${UUID.randomUUID()}"
    )

    @Test
    fun reopeningRetainsTheSameNativeIdentityWithoutReplacingIt() {
        val root = root()
        val first = RetainedTestPeer.prepare(root)
        try {
            val reopened = RetainedTestPeer.reopen(root)
            assertEquals(first.peerId, reopened.peerId)
            assertEquals(first.ready.slot.id, reopened.ready.slot.id)
            assertEquals(listOf(first.ready.slot.id), Slots(root).list().map { it.id })
            assertEquals(first.peerId, reopened.withKey { _, key -> key.peerId() })
            assertThrows(IllegalStateException::class.java) { RetainedTestPeer.prepare(root) }
            assertEquals(first.peerId, RetainedTestPeer.reopen(root).peerId)
        } finally {
            first.remove()
        }
        assertTrue(Slots(root).list().isEmpty())
        assertThrows(IllegalStateException::class.java) { RetainedTestPeer.reopen(root) }
    }

    @Test
    fun missingStateCannotSilentlyGenerateANewIdentity() {
        val root = root()
        assertThrows(IllegalStateException::class.java) { RetainedTestPeer.reopen(root) }
        assertTrue(Slots(root).list().isEmpty())
    }
}
