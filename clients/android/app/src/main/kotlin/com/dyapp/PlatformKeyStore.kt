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
import uniffi.dyapp.KeyStore
import uniffi.dyapp.WrapException

/**
 * The core's `KeyStore` over a non-exportable AES-256-GCM key in the Android Keystore. A wrapped
 * blob is the 12-byte IV followed by the ciphertext and its tag.
 */
object PlatformKeyStore : KeyStore {
    private const val PROVIDER = "AndroidKeyStore"
    private const val ALIAS = "dyapp-identity-wrap"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val IV_SIZE = 12
    private const val TAG_BITS = 128
    private const val KEY_BITS = 256

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

    private fun key(): SecretKey? =
        java.security.KeyStore.getInstance(PROVIDER).apply { load(null) }
            .getKey(ALIAS, null) as SecretKey?

    private fun createKey(): SecretKey {
        val purposes = KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        val spec = KeyGenParameterSpec.Builder(ALIAS, purposes)
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
