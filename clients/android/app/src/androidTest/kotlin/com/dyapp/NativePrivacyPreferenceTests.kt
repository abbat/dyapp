package com.dyapp

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ChatNotice

/** Real registry, Keystore, generated JNI and production preference adapters, without a live node. */
class NativePrivacyPreferenceTests {
    @Test
    fun independentNativePreferencesSurviveReopenWithoutChangingAnotherSelectedIdentity() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val other = fixture.slots.create()
            requireDefaults(first)
            requireDefaults(other)
            RustBridge.withChats(first) {
                it.setPresenceEnabled(false)
                it.setNotice(ChatNotice.NAME)
            }
            write(first, PrivacyPreferenceKind.VIEWS, true)
            assertEquals(listOf(true, false, true), read(first))
            write(first, PrivacyPreferenceKind.TYPING, false)
            assertEquals(listOf(true, false, false), read(first))
            write(first, PrivacyPreferenceKind.RECEIPTS, true)
            assertEquals(listOf(true, true, false), read(first))
            requireDefaults(other)
            assertEquals(other.slot.id, (fixture.slots.open() as Opened.Ready).slot.id)
            val reopened = Slots(fixture.root).choose(first.slot.id) as Opened.Ready
            assertEquals(first.peerId, reopened.peerId)
            assertEquals(listOf(true, true, false), read(reopened))
            RustBridge.withChats(reopened) {
                assertFalse(it.presenceEnabled())
                assertEquals(ChatNotice.NAME, it.notice())
            }
        }
    }

    private fun requireDefaults(active: Opened.Ready) {
        assertEquals(listOf(false, false, true), read(active))
        RustBridge.withChats(active) {
            assertTrue(it.presenceEnabled())
            assertEquals(ChatNotice.FIXED, it.notice())
        }
    }

    private fun read(active: Opened.Ready) = runBlocking {
        PrivacyPreferenceKind.entries.map { kind ->
            withTimeout(PEER_WAIT_MS) { privacyPreferenceCalls(active, kind).resolve(null).getOrThrow() }
        }
    }

    private fun write(active: Opened.Ready, kind: PrivacyPreferenceKind, wanted: Boolean) = runBlocking {
        withTimeout(PEER_WAIT_MS) {
            assertEquals(wanted, privacyPreferenceCalls(active, kind).resolve(wanted).getOrThrow())
        }
    }
}
