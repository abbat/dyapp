package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatMediaCacheEdit
import uniffi.dyapp.ChatMediaCacheException
import uniffi.dyapp.ChatSession
import uniffi.dyapp.IdentityKey
import java.io.File

/** Actual JNI and identity-owned storage; Compose fake backends cannot replace these cases. */
class NativeMediaCacheTests {
    @Test
    fun exactInputAndIndependentEditsPersistAcrossReopenWithoutCrossingIdentities() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val other = fixture.slots.create()
            RustBridge.withChats(owner, ::editCache)
            val reopened = fixture.slots.choose(owner.slot.id) as Opened.Ready
            RustBridge.withChats(reopened) {
                val state = it.mediaCacheSettings()
                assertEquals("1.5", state.limitMib)
                assertFalse(state.state.enabled)
                assertEquals(1_572_864uL, state.state.limitBytes)
            }
            RustBridge.withChats(other) {
                assertEquals("256", it.mediaCacheSettings().limitMib)
                assertTrue(it.mediaCacheSettings().state.enabled)
            }
        }
    }

    private fun editCache(session: ChatSession) {
        val initial = session.mediaCacheSettings()
        assertEquals("1", initial.minMib)
        assertEquals("10240", initial.maxMib)
        val budget = ChatMediaCacheEdit.Budget("1.5")
        val pending = session.changeMediaCache(budget, false)
        assertTrue(pending.confirmationRequired)
        assertEquals(initial, pending.settings)
        val applied = session.changeMediaCache(budget, true)
        assertFalse(applied.confirmationRequired)
        assertEquals("1.5", applied.settings.limitMib)
        assertThrows(ChatMediaCacheException.InvalidLimit::class.java) {
            session.changeMediaCache(ChatMediaCacheEdit.Budget("1.0000001"), true)
        }
        assertEquals(applied.settings, session.mediaCacheSettings())
        val off = ChatMediaCacheEdit.Retention(false)
        assertTrue(session.changeMediaCache(off, false).confirmationRequired)
        val disabled = session.changeMediaCache(off, true)
        assertFalse(disabled.settings.state.enabled)
        assertEquals(1_572_864uL, disabled.settings.state.limitBytes)
    }

    private fun cachedOriginal(owner: Opened.Ready): File = File(owner.slot.dir, "originals/cached").apply {
        val directory = checkNotNull(parentFile)
        assertTrue(directory.isDirectory || directory.mkdirs())
        writeBytes(byteArrayOf(1, 2, 3))
    }

    @Test
    fun actualClearRequiresConfirmationAndKeepsUnrelatedPrivateState() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val contact = IdentityKey.generate().use { it.peerId() }
            RustBridge.withChats(owner) { session ->
                session.setFavorite(contact, true)
                session.setContactNote(contact, "private note")
                val cached = cachedOriginal(owner)
                val before = session.mediaCacheSettings()
                assertEquals(3uL, before.state.usedBytes)
                val pending = session.changeMediaCache(ChatMediaCacheEdit.Clear, false)
                assertTrue(pending.confirmationRequired)
                assertEquals(before, pending.settings)
                assertTrue(cached.readBytes().contentEquals(byteArrayOf(1, 2, 3)))
                val cleared = session.changeMediaCache(ChatMediaCacheEdit.Clear, true)
                assertFalse(cleared.confirmationRequired)
                assertEquals(0uL, cleared.settings.state.usedBytes)
                assertEquals(before.state.limitBytes, cleared.settings.state.limitBytes)
                assertFalse(cached.exists())
                assertEquals("private note", session.contactNote(contact))
                assertTrue(session.favoriteState(contact))
            }
        }
    }
}
