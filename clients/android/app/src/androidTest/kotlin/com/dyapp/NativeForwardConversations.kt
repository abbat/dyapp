package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatMessage
import uniffi.dyapp.ChatState

/** The actual registered app owner and separate native clients of the suite-owned external node. */
internal class NativeForwardConversations(val owner: Opened.Ready) {
    fun establish(peer: TestPeer, text: String) {
        peer.session.sendText(owner.peerId, text)
        PeerTransition("app receives $text").await(
            ready = { history(peer).messages.any { it.author == peer.peerId && it.text == text } },
            advance = { RustBridge.withChats(owner) { it.receive() } }
        )
        val reply = "Accepted: $text"
        RustBridge.withChats(owner) { it.sendText(peer.peerId, reply) }
        peer.awaitMessage(owner.peerId, "app reply to $text") { it.text == reply }
        assertEquals(ChatState.OPEN, peer.session.state(owner.peerId))
        assertEquals(ChatState.OPEN, history(peer).state)
    }

    fun history(peer: TestPeer): ChatHistory = RustBridge.withChats(owner) { it.chatHistory(peer.peerId) }

    fun delivered(target: TestPeer, text: String): ChatHistory {
        target.awaitMessage(owner.peerId, "native forwarded text") { it.forwarded && it.text == text }
        val received = target.session.chatHistory(owner.peerId)
        val copy = received.messages.single { it.author == owner.peerId && it.forwarded && it.text == text }
        val sent = history(target).messages.single { it.author == owner.peerId && it.forwarded && it.text == text }
        assertArrayEquals(sent.id, copy.id)
        assertArrayEquals(sent.groupId, copy.groupId)
        assertTrue(!copy.deleted && !copy.edited)
        return received
    }

    fun source(peer: TestPeer, text: String): ChatMessage =
        history(peer).messages.single { it.author == peer.peerId && it.text == text }
}
