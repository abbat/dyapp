package com.dyapp

import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileNotFoundException

private const val STAGE = "stage"
private const val BEFORE = "before"
private const val FIXTURE = "fixture"
private const val SLOT_ID = "id"
private const val PEER_ID = "peer"
private const val READY = "ready"
private const val CLEANED = "cleaned"

/** Test orchestration only: the registry owns every identity operation and directory. */
internal class WorkerPeerFixture(root: File, checkpoint: File) {
    private val slots = Slots(root)
    private val checkpoint = AtomicFile(checkpoint)

    fun prepare(): Opened.Ready {
        val existing = existing()
        check(existing == null || existing.getString(STAGE) == CLEANED) {
            "An earlier worker fixture needs explicit cleanup"
        }
        val state = JSONObject().put(BEFORE, captureWorkerSelection(slots)).put(STAGE, "preparing")
        save(state)
        val ready = slots.create()
        state.put(FIXTURE, workerSelector(ready)).put(STAGE, READY)
        return runCatching {
            save(state)
            requireWorkerDurability(slots)
            ready
        }.onFailure { failure ->
            runCatching { cleanupState(state) }.exceptionOrNull()?.let(failure::addSuppressed)
        }.getOrThrow()
    }

    fun reopen(): Opened.Ready {
        val state = load()
        check(state.getString(STAGE) == READY) { "Worker fixture is not ready" }
        return verifyWorkerSelector(slots.open(), ownedFixture(state))
    }

    fun cleanup() {
        val state = load()
        if (state.getString(STAGE) == CLEANED) return
        check(state.getString(STAGE) == READY) { "Worker fixture creation has an ambiguous outcome" }
        cleanupState(state)
    }

    private fun cleanupState(state: JSONObject) {
        val fixture = ownedFixture(state)
        val before = state.getJSONObject(BEFORE)
        val removal = runCatching {
            if (slots.list().any { it.id == fixture.getString(SLOT_ID) }) {
                slots.remove(fixture.getString(SLOT_ID), fixture.getString(PEER_ID))
                requireWorkerDurability(slots)
            }
        }
        val restoration = runCatching { restore(before) }
        removal.exceptionOrNull()?.let { failure ->
            restoration.exceptionOrNull()?.let(failure::addSuppressed)
            throw failure
        }
        restoration.getOrThrow()
        save(state.put(STAGE, CLEANED))
    }

    private fun restore(before: JSONObject) {
        when (before.getString(STAGE)) {
            READY -> {
                verifyWorkerSelector(slots.choose(before.getString(SLOT_ID)), before)
                requireWorkerDurability(slots)
            }
            "setup" -> {
                check(slots.open() == Opened.Setup) { "Previous setup state changed" }
            }
            "picker" -> {
                check(slots.open() == Opened.Picker) { "Previous picker state changed" }
            }
            else -> {
                error("Unknown worker checkpoint selection")
            }
        }
        val inventory = before.getJSONArray("inventory")
        val expected = List(inventory.length()) { inventory.getString(it) }
        check(slots.list().map { it.id }.sorted() == expected) { "Unrelated identity inventory changed" }
    }

    private fun ownedFixture(state: JSONObject): JSONObject {
        val fixture = state.getJSONObject(FIXTURE)
        val inventory = state.getJSONObject(BEFORE).getJSONArray("inventory")
        check((0 until inventory.length()).none { inventory.getString(it) == fixture.getString(SLOT_ID) }) {
            "Worker checkpoint points to a pre-existing identity"
        }
        return fixture
    }

    private fun load(): JSONObject =
        checkpoint.openRead().bufferedReader().use { JSONObject(it.readText()) }

    private fun existing(): JSONObject? = try {
        load()
    } catch (failure: FileNotFoundException) {
        if (checkpoint.baseFile.exists()) throw failure
        null
    }

    private fun save(state: JSONObject) {
        val stream = checkpoint.startWrite()
        runCatching {
            stream.write(state.toString().toByteArray(Charsets.UTF_8))
            checkpoint.finishWrite(stream)
        }.onFailure { checkpoint.failWrite(stream) }.getOrThrow()
    }
}

private fun workerSelector(ready: Opened.Ready): JSONObject =
    JSONObject().put(SLOT_ID, ready.slot.id).put(PEER_ID, ready.peerId)

private fun verifyWorkerSelector(opened: Opened, expected: JSONObject): Opened.Ready {
    check(opened is Opened.Ready) { "Expected a ready worker identity" }
    check(opened.slot.id == expected.getString(SLOT_ID) && opened.peerId == expected.getString(PEER_ID)) {
        "Worker identity selectors changed"
    }
    return opened
}

private fun captureWorkerSelection(slots: Slots): JSONObject {
    val before = when (val opened = slots.open()) {
        is Opened.Ready -> workerSelector(opened).put(STAGE, READY)
        Opened.Setup -> JSONObject().put(STAGE, "setup")
        Opened.Picker -> JSONObject().put(STAGE, "picker")
        is Opened.Recovery -> error("Worker tests cannot replace a recovery selection")
    }
    return before.put("inventory", JSONArray(slots.list().map { it.id }.sorted()))
}

private fun requireWorkerDurability(slots: Slots) {
    check(slots.takeDurabilityDetail() == null) { "Worker fixture mutation persistence is unconfirmed" }
}
