package com.dyapp

import android.content.Context
import java.io.File
import uniffi.dyapp.IdentityKey

/** Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings. */
object RustBridge {
    // ponytail: Keystore and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /**
     * Peer ID of this install's identity key. The first call creates the key and stores it
     * wrapped by [PlatformKeyStore] in `noBackupFilesDir`, which backups skip: the wrapping key
     * never leaves this device, so a restored blob could not be opened anyway.
     */
    @Synchronized
    fun peerId(context: Context): String = identity(context).use { it.peerId() }

    private fun identity(context: Context): IdentityKey {
        val file = File(context.noBackupFilesDir, "identity.key")
        if (file.exists()) return IdentityKey.unwrap(PlatformKeyStore, file.readBytes())
        val key = IdentityKey.generate()
        val temp = File(file.path + ".tmp")
        temp.writeBytes(key.wrap(PlatformKeyStore))
        check(temp.renameTo(file)) { "cannot store the identity key" }
        return key
    }
}
