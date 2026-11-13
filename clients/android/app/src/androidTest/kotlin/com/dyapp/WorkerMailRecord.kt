package com.dyapp

import android.util.Base64
import org.json.JSONObject
import java.io.File

private const val WORKER_MESSAGE_ID_BYTES = 16

/** Bind persisted opaque IDs to the exact retained pair, so old runs cannot satisfy new phases. */
internal class WorkerMailRecord(file: File, private val owner: String, private val counterpart: String) {
    private val checkpoint = PeerCheckpoint(file)

    fun begin() = checkpoint.write(JSONObject().put("owner", owner).put("counterpart", counterpart))

    fun record(text: String, id: ByteArray) {
        check(id.size == WORKER_MESSAGE_ID_BYTES) { "Invalid native message ID width" }
        checkpoint.write(read().put(text, Base64.encodeToString(id, Base64.NO_WRAP)))
    }

    fun id(text: String): ByteArray = Base64.decode(read().getString(text), Base64.NO_WRAP).also {
        check(it.size == WORKER_MESSAGE_ID_BYTES) { "Invalid persisted message ID width" }
    }

    private fun read(): JSONObject = checkpoint.read().also {
        check(it.getString("owner") == owner && it.getString("counterpart") == counterpart) {
            "Worker mail checkpoint belongs to a different retained pair"
        }
    }
}
