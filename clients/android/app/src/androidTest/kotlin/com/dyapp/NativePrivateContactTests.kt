package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatState
import uniffi.dyapp.ContactNoteException
import uniffi.dyapp.FavoriteException
import uniffi.dyapp.IdentityKey

/** A native readback, shared by local persistence and authenticated device-import checks. */
internal fun assertNativePrivateContact(owner: Opened.Ready, contact: String, note: String, saved: Boolean) {
    RustBridge.withChats(owner) {
        assertEquals(note, it.contactNote(contact))
        assertEquals(saved, it.favoriteState(contact))
        assertEquals(if (saved) listOf(contact) else emptyList<String>(), it.favoritePeers())
    }
}

/** JNI admission and persistence, without a public profile fetch or a counterpart mailbox. */
class NativePrivateContactTests {
    @Test
    fun privateChangesPersistAndStaySeparateFromPublicDraftLikesAndChats() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val other = fixture.slots.create()
            val contact = IdentityKey.generate().use { it.peerId() }
            val note = "  private \u00fc \u65e5\u672c\u8a9e \uD83D\uDCDA\nkept spacing  "
            saveDraft(owner, ProfileDraft(name = "Public", about = "Public about"))
            val public = loadDraft(owner)
            RustBridge.withChats(owner) { session ->
                val likes = session.likes(contact)
                session.setFavorite(contact, true)
                session.setContactNote(contact, note)
                assertEquals(listOf(contact), session.favoritePeers())
                assertEquals(ChatState.NONE, session.state(contact))
                assertEquals(likes, session.likes(contact))
                assertTrue(session.chatItems(contact).isEmpty())
                assertTrue(session.chatRows().isEmpty())
            }
            val reopened = fixture.slots.choose(owner.slot.id) as Opened.Ready
            assertEquals(owner.peerId, reopened.peerId)
            assertNativePrivateContact(reopened, contact, note, saved = true)
            assertNativePrivateContact(other, contact, "", saved = false)
            assertEquals(public, loadDraft(reopened))
        }
    }

    @Test
    fun typedRefusalsPreservePrivateValuesAndBlockedRemovalStillWorks() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val contact = IdentityKey.generate().use { it.peerId() }
            RustBridge.withChats(owner) { session ->
                assertThrows(FavoriteException.InvalidPeer::class.java) { session.setFavorite("invalid", true) }
                assertThrows(FavoriteException.OwnPeer::class.java) { session.setFavorite(owner.peerId, true) }
                assertThrows(ContactNoteException.InvalidPeer::class.java) { session.setContactNote("invalid", "note") }
                assertThrows(ContactNoteException.OwnPeer::class.java) { session.setContactNote(owner.peerId, "note") }
                session.setFavorite(contact, true)
                session.setContactNote(contact, "retained note")
                assertThrows(ContactNoteException.InvalidText::class.java) { session.setContactNote(contact, "tab\t") }
                assertEquals("retained note", session.contactNote(contact))
                session.block(contact)
                assertTrue(session.favoriteState(contact))
                assertTrue(session.favoritePeers().isEmpty())
                assertThrows(FavoriteException.Blocked::class.java) { session.setFavorite(contact, true) }
                session.setFavorite(contact, false)
                assertFalse(session.favoriteState(contact))
                session.setContactNote(contact, "private while blocked")
                assertEquals("private while blocked", session.contactNote(contact))
                session.setContactNote(contact, "")
            }
            assertNativePrivateContact(owner, contact, "", saved = false)
        }
    }
}
