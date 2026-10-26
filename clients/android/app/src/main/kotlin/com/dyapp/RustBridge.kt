package com.dyapp

import android.content.Context
import android.util.Log

/** What the start screen shows; [opened] is null when the storage itself fails. */
class Shown(
    /** Active identity state, or null when storage failed. */
    val opened: Opened?,
    /** Candidate identities for the picker, including recovery states. */
    val entries: List<Opened> = emptyList()
)

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
    fun show(slots: Slots, action: () -> Opened): Shown = runCatching {
        when (val opened = action()) {
            Opened.Setup -> Shown(slots.create())
            Opened.Picker -> Shown(opened, slots.list().map { openSlot(it, slots::load) })
            else -> Shown(opened)
        }
    }.getOrElse { failure ->
        if (failure !is Exception) throw failure
        Log.e("dyapp", "Identity storage failed", failure)
        Shown(null)
    }
}
