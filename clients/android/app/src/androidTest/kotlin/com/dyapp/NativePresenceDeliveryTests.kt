package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatState

/** Real clients use the suite's existing node; all audience/freshness decisions stay in native core. */
class NativePresenceDeliveryTests {
    @Test
    fun explicitNativePresenceReachesAMatchWithoutImplicitReciprocityOrDisclosureWhileHidden() {
        TestPeer("presence-sender").use { sender ->
            TestPeer("presence-receiver").use { receiver ->
                match(sender, receiver)
                sender.session.setPresenceEnabled(false)
                assertFalse(sender.session.announcePresence())
                receiver.session.receive()
                assertTrue(receiver.session.activeChatPeers().isEmpty())
                sender.session.setPresenceEnabled(true)
                assertTrue(sender.session.announcePresence())
                awaitActive(receiver, sender)
                receiver.session.setPresenceEnabled(false)
                assertEquals(listOf(sender.peerId), receiver.session.activeChatPeers())
                assertFalse(receiver.session.announcePresence())
                sender.session.receive()
                assertTrue(sender.session.activeChatPeers().isEmpty())
                receiver.session.setPresenceEnabled(true)
                assertTrue(receiver.session.announcePresence())
                awaitActive(sender, receiver)
            }
        }
    }

    private fun match(sender: TestPeer, receiver: TestPeer) {
        sender.session.like(receiver.peerId, true)
        receiver.session.receive()
        receiver.session.like(sender.peerId, true)
        sender.session.receive()
        assertEquals(ChatState.OPEN, sender.session.state(receiver.peerId))
        assertEquals(ChatState.OPEN, receiver.session.state(sender.peerId))
    }

    private fun awaitActive(receiver: TestPeer, sender: TestPeer) {
        PeerTransition("native presence from " + sender.peerId).await(
            ready = { sender.peerId in receiver.session.activeChatPeers() },
            advance = { receiver.session.receive() }
        )
        assertEquals(listOf(sender.peerId), receiver.session.activeChatPeers())
    }
}
