package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Explicit host-run phases; ordinary instrumentation must not leave a retained fixture behind. */
class RetainedPeerPhaseTests {
    @Test
    fun runRequestedPhase() {
        val phase = InstrumentationRegistry.getArguments().getString("retainedPeerPhase")
        assumeTrue(phase != null)
        val files = InstrumentationRegistry.getInstrumentation().targetContext.filesDir
        val root = File(files, "retained-background-peer")
        val expected = File(files, "retained-background-peer.expected")
        when (phase) {
            "prepare" -> {
                val peer = RetainedTestPeer.prepare(root)
                assertTrue(peer.withChats { it.chatRows() }.isEmpty())
                expected.writeText("${peer.ready.slot.id}\n${peer.peerId}\n")
            }
            "reopen" -> {
                val peer = RetainedTestPeer.reopen(root)
                assertEquals("${peer.ready.slot.id}\n${peer.peerId}\n", expected.readText())
                assertEquals(peer.peerId, peer.withKey { _, key -> key.peerId() })
                assertTrue(peer.withChats { it.chatRows() }.isEmpty())
            }
            "cleanup" -> {
                val peer = RetainedTestPeer.reopen(root)
                assertEquals("${peer.ready.slot.id}\n${peer.peerId}\n", expected.readText())
                peer.remove()
                assertEquals(Opened.Setup, Slots(root).open())
            }
            else -> {
                error("Unknown retained peer phase: $phase")
            }
        }
    }
}
