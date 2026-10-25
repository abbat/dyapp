package com.dyapp

import android.content.Context

/** Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings. */
object RustBridge {
    // ponytail: Keystore and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /**
     * Peer ID of the active identity. The first launch creates one in a slot under
     * `noBackupFilesDir`, which backups skip: its wrapping key never leaves this device, so a
     * restored blob could not be opened anyway. An upgrade keeps the root key as the legacy slot.
     */
    @Synchronized
    fun peerId(context: Context): String =
        when (val opened = slots(context).open()) {
            is Opened.Ready -> opened.peerId
            Opened.Setup -> slots(context).create().peerId
            // shortcut: no picker or recovery screen yet, so these fail visibly; the identity
            // screens replace this.
            Opened.Picker -> error("Several identities and no active one")
            is Opened.Recovery -> error("Identity needs recovery: ${opened.reason}")
        }

    fun slots(context: Context) = Slots(context.noBackupFilesDir, PlatformKeyStore)
}
