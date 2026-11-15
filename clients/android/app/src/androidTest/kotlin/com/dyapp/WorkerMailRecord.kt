package com.dyapp

import android.os.Process
import android.provider.Settings
import android.util.Base64
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import java.io.File

private const val WORKER_MESSAGE_ID_BYTES = 16

/** Public selectors and a digest only; original bytes stay in the native sealed cache. */
internal data class WorkerPhotoProof(val selector: String, val sha256: String, val width: Int, val height: Int) {
    init {
        check(selector.isNotBlank() && Regex("[0-9a-f]{64}").matches(sha256) && width > 0 && height > 0) {
            "Invalid worker photo proof"
        }
    }
}

/** Bind persisted opaque IDs to the exact retained pair, so old runs cannot satisfy new phases. */
internal class WorkerMailRecord(
    file: File,
    private val owner: String,
    private val counterpart: String,
    private val process: () -> String = ::workerProcessMarker,
) {
    private val checkpoint = PeerCheckpoint(file)

    fun begin() = checkpoint.write(
        JSONObject().put("owner", owner).put("counterpart", counterpart).put("preparationProcess", process())
    )

    fun requireFreshProcess() {
        check(read().getString("preparationProcess") != process()) {
            "Worker body reused the preparation process and its proof overrides"
        }
    }

    fun record(text: String, id: ByteArray) {
        check(id.size == WORKER_MESSAGE_ID_BYTES) { "Invalid native message ID width" }
        checkpoint.write(read().put(text, Base64.encodeToString(id, Base64.NO_WRAP)))
    }

    fun id(text: String): ByteArray = Base64.decode(read().getString(text), Base64.NO_WRAP).also {
        check(it.size == WORKER_MESSAGE_ID_BYTES) { "Invalid persisted message ID width" }
    }

    fun recordPhoto(photo: WorkerPhotoProof) {
        val value = JSONObject()
            .put("selector", photo.selector)
            .put("sha256", photo.sha256)
            .put("width", photo.width)
            .put("height", photo.height)
        checkpoint.write(read().put("cachedPhoto", value))
    }

    fun photo(): WorkerPhotoProof {
        val value = read().getJSONObject("cachedPhoto")
        return WorkerPhotoProof(
            value.getString("selector"),
            value.getString("sha256"),
            value.photoDimension("width"),
            value.photoDimension("height")
        )
    }

    private fun read(): JSONObject = checkpoint.read().also {
        check(it.getString("owner") == owner && it.getString("counterpart") == counterpart) {
            "Worker mail checkpoint belongs to a different retained pair"
        }
    }
}

private fun JSONObject.photoDimension(key: String): Int = getLong(key).also {
    check(it in 1L..Int.MAX_VALUE.toLong()) { "Invalid worker photo dimension" }
}.toInt()

// Boot count separates reboots; the start timestamp also protects against same-boot PID reuse.
internal fun workerProcessMarker(): String {
    val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
    val boot = Settings.Global.getInt(resolver, Settings.Global.BOOT_COUNT)
    return "$boot:${Process.myPid()}:${Process.getStartElapsedRealtime()}"
}
