package com.dyapp

import android.util.AtomicFile
import org.json.JSONObject
import java.io.File
import java.io.FileNotFoundException

/** App-private test metadata contains public selectors and message IDs, never credentials. */
internal class PeerCheckpoint(file: File) {
    private val atomic = AtomicFile(file)

    fun read(): JSONObject = atomic.openRead().bufferedReader().use { JSONObject(it.readText()) }

    fun existing(): JSONObject? = try {
        read()
    } catch (failure: FileNotFoundException) {
        if (atomic.baseFile.exists()) throw failure
        null
    }

    fun write(state: JSONObject) {
        val stream = atomic.startWrite()
        runCatching {
            stream.write(state.toString().toByteArray(Charsets.UTF_8))
            atomic.finishWrite(stream)
        }.onFailure { atomic.failWrite(stream) }.getOrThrow()
    }
}
