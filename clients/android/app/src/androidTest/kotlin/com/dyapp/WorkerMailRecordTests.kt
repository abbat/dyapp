package com.dyapp

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONException
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
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
    fun aPhotoProofReopensOnlyForItsOriginalRetainedPair() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        val proof = WorkerPhotoProof("group:author:id", "ab".repeat(32), 3, 2)
        record.begin()
        record.recordPhoto(proof)
        assertEquals(proof, WorkerMailRecord(file, "owner", "other").photo())
        val swapped = WorkerMailRecord(file, "other", "owner")
        assertThrows(IllegalStateException::class.java) { swapped.photo() }
        assertThrows(IllegalStateException::class.java) { swapped.recordPhoto(proof) }
        assertEquals(proof, record.photo())
    }

    @Test
    fun missingOrMalformedPhotoProofCannotInitializeADefault() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        record.begin()
        assertThrows(JSONException::class.java) { record.photo() }
        val checkpoint = PeerCheckpoint(file)
        for ((field, value) in listOf("sha256" to "short", "width" to 0, "height" to Long.MAX_VALUE)) {
            val photo = JSONObject()
                .put("selector", "group:author:id")
                .put("sha256", "ab".repeat(32))
                .put("width", 3)
                .put("height", 2)
                .put(field, value)
            checkpoint.write(checkpoint.read().put("cachedPhoto", photo))
            assertThrows(IllegalStateException::class.java) { record.photo() }
        }
    }

    @Test
    fun aMissingRecordFailsWithoutInitializingIt() {
        val file = file()
        val record = WorkerMailRecord(file, "owner", "other")
        assertThrows(FileNotFoundException::class.java) { record.id(WORKER_ANCHOR) }
        assertFalse(file.exists())
    }
}
