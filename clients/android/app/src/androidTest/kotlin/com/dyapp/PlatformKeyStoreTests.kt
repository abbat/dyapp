package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.WrapException
import java.util.UUID

class PlatformKeyStoreTests {
    @Test
    fun anExistingOnlyStoreNeverCreatesAnAbsentCredential() {
        val alias = "dyapp-test-existing:${UUID.randomUUID()}"
        try {
            val store = PlatformKeyStore.existing(alias)
            val secret = ByteArray(32) { 7 }
            val failure = runCatching { store.wrap(secret) }.exceptionOrNull()
            assertTrue(failure is WrapException.Failed)
            assertArrayEquals(ByteArray(32), secret)
            assertFalse(PlatformKeyStore.exists(alias))
        } finally {
            PlatformKeyStore.delete(alias)
        }
    }

    @Test
    fun anExistingOnlyStoreReusesTheCredentialAndRefusesItsLoss() {
        val alias = "dyapp-test-existing:${UUID.randomUUID()}"
        try {
            val original = ByteArray(32) { 11 }
            val wrapped = PlatformKeyStore(alias).wrap(original.copyOf())
            val store = PlatformKeyStore.existing(alias)
            assertArrayEquals(original, store.unwrap(wrapped))
            assertArrayEquals(original, store.unwrap(store.wrap(original.copyOf())))
            PlatformKeyStore.delete(alias)
            val secret = original.copyOf()
            assertTrue(runCatching { store.wrap(secret) }.exceptionOrNull() is WrapException.Failed)
            assertArrayEquals(ByteArray(32), secret)
            assertTrue(runCatching { store.unwrap(wrapped) }.exceptionOrNull() is WrapException.Failed)
            assertFalse(PlatformKeyStore.exists(alias))
        } finally {
            PlatformKeyStore.delete(alias)
        }
    }
}
