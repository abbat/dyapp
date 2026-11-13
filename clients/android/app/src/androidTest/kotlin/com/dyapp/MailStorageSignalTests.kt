package com.dyapp

import android.os.FileObserver
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.UUID

class MailStorageSignalTests {
    @Test
    fun aRealWriteSignalsWithoutAReadinessSleep() {
        val directory = File(
            InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
            "mail-signal-${UUID.randomUUID()}"
        ).apply { check(mkdir()) }
        val data = File(directory, "data").apply { writeText("") }
        MailStorageSignal(directory).use { signal ->
            data.appendText("committed")
            signal.await(PEER_WAIT_MS)
            assertEquals("committed", data.readText())
        }
    }

    @Test
    fun readsAndSharedMemoryChangesDoNotCountAsMailStorageProgress() {
        assertFalse(mailStorageChanged(FileObserver.OPEN, "data"))
        assertFalse(mailStorageChanged(FileObserver.CLOSE_WRITE, "data"))
        assertFalse(mailStorageChanged(FileObserver.MODIFY, "data-shm"))
        assertTrue(mailStorageChanged(FileObserver.MODIFY, "data-wal"))
        assertTrue(mailStorageChanged(FileObserver.MODIFY, "data"))
    }
}
