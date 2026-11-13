package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import java.io.File
import java.util.UUID

class NativeSlotsTests {
    @Test
    fun createSwitchAndReopenUseTheSameVerifiedNativeIdentities() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val second = fixture.slots.create()
            assertTrue(first.peerId != second.peerId)
            assertEquals(second.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            assertEquals(first.peerId, (fixture.slots.choose(first.slot.id) as Opened.Ready).peerId)
            assertEquals(first.peerId, (Slots(fixture.root).open() as Opened.Ready).peerId)
        }
    }

    @Test
    fun pendingRecoveryAndRetryNeverReplaceTheSavedSelectionOrGenerateAnAlias() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val id = UUID.randomUUID().toString()
            assertFalse(PlatformKeyStore.exists(identityAlias(id)))
            val dir = File(fixture.root, "profiles/$id").apply { check(mkdir()) }
            File(dir, "state").writeText("pending\n")
            val recovery = fixture.slots.choose(id) as Opened.Recovery
            assertEquals("incomplete", recovery.reason)
            assertEquals(dir, recovery.slot?.dir)
            assertTrue(fixture.slots.retry(recovery) is Opened.Recovery)
            assertEquals(first.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            assertFalse(PlatformKeyStore.exists(identityAlias(id)))
        }
    }

    @Test
    fun aForeignFileRefusesRemovalBeforeErasingTheCredential() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            val key = File(ready.slot.dir, "identity.key")
            val saved = key.readBytes()
            File(ready.slot.dir, "notes.txt").writeText("keep")
            assertThrows(CoreException::class.java) { fixture.slots.remove(ready.slot.id, ready.peerId) }
            assertTrue(PlatformKeyStore.exists(identityAlias(ready.slot.id)))
            assertArrayEquals(saved, key.readBytes())
            assertEquals(ready.peerId, (fixture.slots.open() as Opened.Ready).peerId)
        }
    }

    @Test
    fun confirmedRemovalKeepsTheOtherIdentityAndItsCredential() {
        NativeSlotFixture().use { fixture ->
            val first = fixture.slots.create()
            val second = fixture.slots.create()
            assertThrows(CoreException::class.java) { fixture.slots.remove(second.slot.id, first.peerId) }
            fixture.slots.remove(second.slot.id, second.peerId)
            assertEquals(listOf(first.slot.id), fixture.slots.list().map { it.id })
            assertTrue(PlatformKeyStore.exists(identityAlias(first.slot.id)))
            assertFalse(PlatformKeyStore.exists(identityAlias(second.slot.id)))
        }
    }

    @Test
    fun missingCredentialsRemainRecoveryAndCanBeCleanedWithoutReplacement() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            PlatformKeyStore.delete(identityAlias(ready.slot.id))
            val recovery = fixture.slots.open() as Opened.Recovery
            assertEquals("missing", recovery.reason)
            assertFalse(PlatformKeyStore.exists(identityAlias(ready.slot.id)))
            fixture.slots.remove(ready.slot.id, null)
            assertTrue(fixture.slots.list().isEmpty())
        }
    }

    @Test
    fun aMissingPointerTargetRemainsRecoveryWithoutInventingACandidate() {
        NativeSlotFixture().use { fixture ->
            check(fixture.root.mkdirs())
            val id = UUID.randomUUID().toString()
            File(fixture.root, "active-profile").writeText("$id\n")
            val recovery = fixture.slots.open() as Opened.Recovery
            assertEquals(id, recovery.id)
            assertNull(recovery.slot)
            assertFalse(removable(recovery))
            val retried = fixture.slots.retry(recovery) as Opened.Recovery
            assertEquals(id, retried.id)
            assertNull(retried.slot)
            assertEquals("$id\n", File(fixture.root, "active-profile").readText())
            assertFalse(PlatformKeyStore.exists(identityAlias(id)))
            assertTrue(fixture.slots.list().isEmpty())
        }
    }

    @Test
    fun malformedPointersNeedNoFabricatedLocationOrRemovalAction() {
        NativeSlotFixture().use { fixture ->
            check(fixture.root.mkdirs())
            File(fixture.root, "active-profile").writeText("../escape\n")
            val recovery = fixture.slots.open() as Opened.Recovery
            assertEquals("invalidPointer", recovery.reason)
            assertNull(recovery.slot)
            assertFalse(removable(recovery))
            assertTrue(fixture.slots.retry(recovery) is Opened.Recovery)
            assertTrue(fixture.slots.list().isEmpty())
        }
    }
}
