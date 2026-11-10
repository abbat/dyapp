package com.dyapp

import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileSlotKeys
import uniffi.dyapp.WrapException

/** Existing Android alias names are a storage contract, including the legacy root identity. */
internal fun identityAlias(slotId: String): String =
    if (slotId == Slots.LEGACY) "dyapp-identity-wrap" else "dyapp-identity-wrap:$slotId"

/** Platform credentials for the common registry; Rust owns all slot files and lifecycle policy. */
internal object NativeSlotKeys : MobileSlotKeys {
    override fun contains(slotId: String): Boolean = credentialCall {
        PlatformKeyStore.exists(identityAlias(slotId))
    }

    override fun openWrapped(slotId: String, wrapped: ByteArray): IdentityKey? = credentialCall {
        val alias = identityAlias(slotId)
        if (PlatformKeyStore.exists(alias)) IdentityKey.unwrap(PlatformKeyStore(alias), wrapped) else null
    }

    override fun wrap(slotId: String, key: IdentityKey): ByteArray = key.use { owned ->
        credentialCall {
            val alias = identityAlias(slotId)
            if (PlatformKeyStore.exists(alias)) throw WrapException.Failed("the wrapping credential already exists")
            owned.wrap(PlatformKeyStore(alias))
        }
    }

    override fun erase(slotId: String) = credentialCall {
        PlatformKeyStore.delete(identityAlias(slotId))
    }
}

// Callback failures use the declared native error type; fatal VM errors still propagate.
private fun <T> credentialCall(action: () -> T): T = runCatching(action).getOrElse { failure ->
    if (failure !is Exception || failure is WrapException) throw failure
    throw WrapException.Failed(failure.javaClass.simpleName + ": " + failure.message)
}
