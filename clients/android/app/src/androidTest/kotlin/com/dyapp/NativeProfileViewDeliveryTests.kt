package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatNotice
import uniffi.dyapp.ChatState
import uniffi.dyapp.ReceivedProfileView

/** Actual first-contact JNI delivery; no Like or chat is used to establish the private channel. */
class NativeProfileViewDeliveryTests {
    @Test
    fun anExplicitViewReachesADisabledRecipientWithoutLikeChatOrNotice() {
        TestPeer("view-sender").use { sender ->
            TestPeer("view-receiver").use { receiver ->
                assertFalse(receiver.session.profileViewsEnabled())
                receiver.session.setNotice(ChatNotice.TEXT)
                assertEquals(ChatNotice.TEXT, receiver.session.notice())
                refuseUnadmitted(sender, receiver)
                sender.session.profileViewOpening(receiver.peerId).use { opening ->
                    assertNotNull(sender.session.submitProfileView(opening))
                    assertNull(sender.session.submitProfileView(opening))
                    val row = awaitView(sender, receiver)
                    assertEquals(sender.peerId, row.peerId)
                    assertTrue(row.utcDate.matches(Regex("\\d{4}-\\d{2}-\\d{2}")))
                    assertNull(sender.session.submitProfileView(opening))
                    assertEquals(listOf(row), receiver.session.receivedProfileViews())
                }
                assertFalse(receiver.session.profileViewsEnabled())
                assertTrue(sender.session.receivedProfileViews().isEmpty())
                assertUnchangedRelationships(sender, receiver)
                assertUnchangedRelationships(receiver, sender)
            }
        }
    }

    private fun refuseUnadmitted(sender: TestPeer, receiver: TestPeer) {
        assertFalse(sender.session.profileViewsEnabled())
        assertTrue(receiver.session.receivedProfileViews().isEmpty())
        sender.session.profileViewOpening(receiver.peerId).use { disabled ->
            assertNull(sender.session.submitProfileView(disabled))
            sender.session.setProfileViewsEnabled(true)
            assertNull(sender.session.submitProfileView(disabled))
        }
        sender.session.profileViewOpening(receiver.peerId).use { retired ->
            retired.retire()
            assertNull(sender.session.submitProfileView(retired))
        }
        assertTrue(receiver.session.receivedProfileViews().isEmpty())
    }

    private fun awaitView(sender: TestPeer, receiver: TestPeer): ReceivedProfileView {
        var rows = emptyList<ReceivedProfileView>()
        val label = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.notice_sticker)
        PeerTransition("native first profile view").await(
            ready = {
                rows = receiver.session.receivedProfileViews()
                rows.any { it.peerId == sender.peerId }
            },
            advance = {
                assertNull(sender.session.receiveWithNotice(label).notice)
                assertNull(receiver.session.receiveWithNotice(label).notice)
            }
        )
        assertEquals(1, rows.size)
        return rows.single()
    }

    private fun assertUnchangedRelationships(owner: TestPeer, other: TestPeer) {
        val likes = owner.session.likes(other.peerId)
        assertFalse(likes.ours)
        assertFalse(likes.matched)
        assertTrue(owner.session.likedYou().isEmpty())
        assertEquals(ChatState.NONE, owner.session.state(other.peerId))
        assertTrue(owner.session.chatRows().isEmpty())
    }
}
