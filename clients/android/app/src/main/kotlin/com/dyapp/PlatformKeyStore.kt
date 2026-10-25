package com.dyapp

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.ProviderException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.KeyStore
import uniffi.dyapp.WrapException

/**
 * The core's `KeyStore` over the non-exportable AES-256-GCM key [alias] in the Android Keystore,
 * one per identity. A wrapped blob is the 12-byte IV followed by the ciphertext and its tag.
 */
class PlatformKeyStore(private val alias: String) : KeyStore {
    companion object : SlotKeys {
        private const val PROVIDER = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_SIZE = 12
        private const val TAG_BITS = 128
        private const val KEY_BITS = 256

        private fun keystore() = java.security.KeyStore.getInstance(PROVIDER).apply { load(null) }

        override fun exists(alias: String): Boolean = keystore().containsAlias(alias)

        override fun generate(alias: String): Pair<ByteArray, String> =
            IdentityKey.generate().use { Pair(it.wrap(PlatformKeyStore(alias)), it.peerId()) }

        override fun peerId(alias: String, wrapped: ByteArray): String =
            IdentityKey.unwrap(PlatformKeyStore(alias), wrapped).use { it.peerId() }

        override fun delete(alias: String) = keystore().deleteEntry(alias)
    }

    override fun wrap(secret: ByteArray): ByteArray = keystoreCall {
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, key() ?: createKey())
            cipher.iv + cipher.doFinal(secret)
        } finally {
            secret.fill(0)
        }
    }

    override fun unwrap(wrapped: ByteArray): ByteArray = keystoreCall {
        // A missing key is an error, not a reason to make a new one: the blob stays unreadable.
        val key = key() ?: throw WrapException.Failed("the wrapping key is gone")
        if (wrapped.size <= IV_SIZE) throw WrapException.Failed("wrapped key is too short")
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, wrapped, 0, IV_SIZE))
        cipher.doFinal(wrapped, IV_SIZE, wrapped.size - IV_SIZE)
    }

    private fun key(): SecretKey? = keystore().getKey(alias, null) as SecretKey?

    private fun createKey(): SecretKey {
        val purposes = KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        val spec = KeyGenParameterSpec.Builder(alias, purposes)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(KEY_BITS)
            .build()
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(spec)
        return generator.generateKey()
    }

    private fun keystoreCall(block: () -> ByteArray): ByteArray =
        try {
            block()
        } catch (error: GeneralSecurityException) {
            throw failed(error)
        } catch (error: IOException) {
            throw failed(error)
        } catch (error: ProviderException) {
            throw failed(error)
        }

    // The core expects `WrapException`; the reason names the failure, never key bytes.
    private fun failed(error: Exception) =
        WrapException.Failed(error.javaClass.simpleName + ": " + error.message)
}
