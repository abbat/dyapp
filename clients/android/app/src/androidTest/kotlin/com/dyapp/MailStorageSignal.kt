package com.dyapp

import android.os.FileObserver
import java.io.File
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit

/** Filesystem callbacks wake history reads; an event itself never proves mail delivery. */
internal class MailStorageSignal(directory: File) : AutoCloseable {
    private val changes = ArrayBlockingQueue<Unit>(1)
    private val observer = object : FileObserver(directory, FileObserver.MODIFY) {
        override fun onEvent(event: Int, path: String?) {
            if (mailStorageChanged(event, path)) changes.offer(Unit)
        }
    }

    init {
        observer.startWatching()
    }

    fun await(remainingMs: Long) {
        check(changes.poll(remainingMs, TimeUnit.MILLISECONDS) != null) {
            "No mail storage change before the transition deadline"
        }
    }

    override fun close() = observer.stopWatching()
}

// Read-only SQLite connections can maintain SHM; those changes are not mailbox progress.
internal fun mailStorageChanged(event: Int, path: String?): Boolean =
    event and FileObserver.MODIFY != 0 && path?.endsWith("-shm") != true
