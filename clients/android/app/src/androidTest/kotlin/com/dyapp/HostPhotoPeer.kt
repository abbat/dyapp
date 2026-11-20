package com.dyapp

import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.net.ConnectException
import java.net.InetSocketAddress
import java.net.Socket

private const val PHOTO_HOST_PORT = 33_446
private const val PHOTO_HOST_REPLY_BYTES = 4_096

private fun connectPhotoHost(): Socket {
    var connected: Socket? = null
    PeerTransition("host PNG listener").awaitSignal({ connected != null }) { remaining ->
        val candidate = Socket()
        runCatching {
            candidate.connect(InetSocketAddress("10.0.2.2", PHOTO_HOST_PORT), remaining.toInt())
            candidate
        }.onSuccess { connected = it }.onFailure {
            candidate.close()
            // A refused dial is a completed not-ready signal; never retry an uncertain connection.
            if (it !is ConnectException) throw it
        }
    }
    return checkNotNull(connected)
}

/** Test-only guest control of an external native client; never an Android network node. */
internal class HostPhotoPeer(owner: String) : AutoCloseable {
    private val socket = connectPhotoHost()
    private val input = socket.getInputStream().buffered()
    val peerId: String = runCatching {
        send(JSONObject().put("op", "init").put("peer", owner))
        check(read("host PNG initialization").getBoolean("initialized"))
        val ready = read("host PNG profile and key packages")
        check(ready.getBoolean("ready"))
        ready.getString("peer")
    }.onFailure { socket.close() }.getOrThrow()

    fun receive(): JSONObject = command(JSONObject().put("op", "receive"))

    fun text(): JSONObject = command(JSONObject().put("op", "text"))

    fun photo(bytes: ByteArray, width: UInt, height: UInt): JSONObject {
        val encoded = JSONArray()
        bytes.forEach { encoded.put(it.toInt() and 0xff) }
        return command(
            JSONObject().put("op", "photo").put("bytes", encoded)
                .put("width", width.toLong())
                .put("height", height.toLong())
        )
    }

    private fun command(value: JSONObject): JSONObject {
        send(value)
        return read("host PNG command completion")
    }

    private fun send(value: JSONObject) {
        socket.getOutputStream().write((value.toString() + "\n").toByteArray(Charsets.UTF_8))
        socket.getOutputStream().flush()
    }

    private fun read(what: String): JSONObject {
        val bytes = ByteArrayOutputStream()
        var complete = false
        PeerTransition(what).awaitSignal({ complete }) { remaining ->
            socket.soTimeout = remaining.toInt()
            val byte = input.read()
            check(byte >= 0) { "Host PNG client closed before $what" }
            if (byte == '\n'.code) {
                complete = true
            } else {
                check(bytes.size() < PHOTO_HOST_REPLY_BYTES) { "Oversized host PNG reply" }
                bytes.write(byte)
            }
        }
        return JSONObject(bytes.toString(Charsets.UTF_8.name()))
    }

    override fun close() {
        try {
            check(command(JSONObject().put("op", "quit")).getBoolean("closed"))
        } finally {
            socket.close()
        }
    }
}

internal fun JSONObject.photoHostBytes(name: String, expected: Int): ByteArray {
    val values = getJSONArray(name)
    check(values.length() == expected)
    return ByteArray(expected) { index ->
        val value = values.getInt(index)
        check(value in 0..UByte.MAX_VALUE.toInt())
        value.toByte()
    }
}
