package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.WrapException
import java.util.UUID

class NativeSlotKeyWrappingTests {
    @Test
    fun existingWrapNeverCreatesAMissingSlotCredential() {
        val slot = UUID.randomUUID().toString()
        val key = IdentityKey.generate()
        val failure = runCatching { NativeSlotKeys.wrapExisting(slot, key) }.exceptionOrNull()
        assertTrue(failure is WrapException.Failed)
        assertFalse(NativeSlotKeys.contains(slot))
    }

    @Test
    fun existingWrapKeepsTheOwnerAndReopensTheDevice() {
        val slot = UUID.randomUUID().toString()
        try {
            val owner = IdentityKey.generate()
            val ownerPeer = owner.peerId()
            val wrappedOwner = NativeSlotKeys.wrap(slot, owner)
            val device = IdentityKey.generate()
            val devicePeer = device.peerId()
            val wrappedDevice = NativeSlotKeys.wrapExisting(slot, device)
            NativeSlotKeys.openWrapped(slot, wrappedDevice).use { assertEquals(devicePeer, it?.peerId()) }
            NativeSlotKeys.openWrapped(slot, wrappedOwner).use { assertEquals(ownerPeer, it?.peerId()) }
            NativeSlotKeys.erase(slot)
            val failure = runCatching {
                NativeSlotKeys.wrapExisting(slot, IdentityKey.generate())
            }.exceptionOrNull()
            assertTrue(failure is WrapException.Failed)
            assertFalse(NativeSlotKeys.contains(slot))
        } finally {
            NativeSlotKeys.erase(slot)
        }
    }
}
