package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.File
import java.io.FileNotFoundException
import java.util.UUID

class WorkerMailRecordTests {
    private fun file() = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "worker-mail-${UUID.randomUUID()}.json"
    )

    @Test
    fun exactOpaqueIdsReopenOnlyForTheSamePair() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        val id = ByteArray(16) { (-it).toByte() }
        record.begin()
        record.record(WORKER_ANCHOR, id)
        assertArrayEquals(id, WorkerMailRecord(file, "owner", "other").id(WORKER_ANCHOR))
        val swapped = WorkerMailRecord(file, "other", "owner")
        assertThrows(IllegalStateException::class.java) { swapped.id(WORKER_ANCHOR) }
        assertThrows(IllegalStateException::class.java) { swapped.record(WORKER_ANCHOR, ByteArray(16)) }
        assertArrayEquals(id, record.id(WORKER_ANCHOR))
    }

    @Test
    fun invalidIdsAndAbsentEntriesCannotBecomeAReplacementId() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        record.begin()
        assertThrows(IllegalStateException::class.java) { record.record(WORKER_ANCHOR, byteArrayOf()) }
        assertThrows(JSONException::class.java) { record.id(WORKER_ANCHOR) }
        val checkpoint = PeerCheckpoint(file)
        checkpoint.write(checkpoint.read().put(WORKER_ANCHOR, "AA=="))
        assertThrows(IllegalStateException::class.java) { record.id(WORKER_ANCHOR) }
    }

    @Test
    fun aFreshProcessMustDifferFromTheRecordedPreparationProcess() {
        val file = file()
        var process = "preparation"
        val record = WorkerMailRecord(file, "owner", "other") { process }
        record.begin()
        assertThrows(IllegalStateException::class.java) { record.requireFreshProcess() }
        process = "receiver"
        WorkerMailRecord(file, "owner", "other") { process }.requireFreshProcess()
    }

    @Test
    fun aMissingRecordFailsWithoutInitializingIt() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        assertThrows(FileNotFoundException::class.java) { record.id(WORKER_ANCHOR) }
        assertFalse(file.exists())
    }
}
