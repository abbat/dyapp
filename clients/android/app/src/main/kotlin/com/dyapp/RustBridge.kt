package com.dyapp

import android.content.Context
import android.util.Log

/** What the start screen shows; [opened] is null when the storage itself fails. */
class Shown(val opened: Opened?, val entries: List<Opened> = emptyList())

/** Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings. */
object RustBridge {
    /**
     * Identity slots under `noBackupFilesDir`, which backups skip: their wrapping keys never
     * leave this device, so a restored blob could not be opened anyway.
     */
    fun slots(context: Context) = Slots(context.noBackupFilesDir, PlatformKeyStore)

    // ponytail: Keystore and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /**
     * Runs [action] and returns the screen for its result. A fresh install gets its first
     * identity; the picker lists every slot, opened or with its recovery reason.
     */
    @Synchronized
    fun show(slots: Slots, action: () -> Opened): Shown = try {
        when (val opened = action()) {
            Opened.Setup -> Shown(slots.create())
            Opened.Picker -> Shown(opened, slots.list().map(slots::openSlot))
            else -> Shown(opened)
        }
    } catch (e: Exception) {
        Log.e("dyapp", "Identity storage failed", e)
        Shown(null)
    }
}
