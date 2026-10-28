package com.dyapp

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.util.UUID

private const val IDENTITY_KEY_FILE = "identity.key"
private const val SLOT_STATE_FILE = "state"
private const val MAX_MARKER_BYTES = 64

private fun isRegularFile(file: File) =
    Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS)

private fun exists(file: File) = Files.exists(file.toPath(), LinkOption.NOFOLLOW_LINKS)

// Reads a short marker file; null if absent, an error if it is not a regular file.
private fun readMarker(file: File): String? {
    if (!exists(file)) return null
    if (!isRegularFile(file)) throw IOException("Marker is not a regular file")
    val bytes = ByteArray(MAX_MARKER_BYTES)
    val size = file.inputStream().use { it.read(bytes) }
    return String(bytes, 0, maxOf(size, 0))
}

// Replaces the file by a flushed temporary file and a same-directory rename.
private fun writeAtomic(file: File, value: ByteArray) {
    val temp = File(file.path + ".tmp")
    temp.delete()
    FileOutputStream(temp).use {
        it.write(value)
        it.fd.sync()
    }
    Files.move(temp.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE)
    FileChannel.open(file.parentFile!!.toPath(), StandardOpenOption.READ).use { it.force(true) }
}

private fun readSlot(id: String, dir: File): Slot {
    val marker = if (Files.isDirectory(dir.toPath(), LinkOption.NOFOLLOW_LINKS)) {
        runCatching { readMarker(File(dir, SLOT_STATE_FILE)) }.getOrNull()
    } else {
        null
    }
    val state = SlotState.entries.firstOrNull { it.marker == marker } ?: SlotState.BROKEN
    return Slot(id, dir, state)
}

/** Open one listed slot; Keystore or unwrap failures become recoverable unavailable state. */
internal fun openSlot(slot: Slot, load: (Slot) -> String?): Opened = runCatching {
    when (slot.state) {
        SlotState.READY -> load(slot)?.let { Opened.Ready(slot, it) }
            ?: Opened.Recovery(slot, "missing", null)
        SlotState.PENDING -> Opened.Recovery(slot, "incomplete", load(slot))
        SlotState.DELETING -> Opened.Recovery(slot, "deleting", load(slot))
        SlotState.BROKEN -> Opened.Recovery(slot, "damaged", null)
    }
}.getOrElse { failure ->
    if (failure !is Exception) throw failure
    Opened.Recovery(slot, "unavailable", null)
}

/** Wrapping keys by Keystore alias; the app uses [PlatformKeyStore], JVM tests a fake. */
interface SlotKeys {
    /** Whether a wrapping key already exists at [alias]. */
    fun exists(alias: String): Boolean

    /** A new identity wrapped by a new key at [alias], and its Peer ID. */
    fun generate(alias: String): Pair<ByteArray, String>

    /** Peer ID obtained by opening [wrapped] with the key at [alias]. */
    fun peerId(alias: String, wrapped: ByteArray): String

    /** Remove the wrapping key at [alias] after slot removal was confirmed. */
    fun delete(alias: String)
}

/** Lifecycle marker for a local identity slot. */
enum class SlotState(
    /** Exact on-disk state marker, or null for damaged metadata. */
    val marker: String?
) {
    /** Creation was interrupted or is still in progress. */
    PENDING("pending\n"),
    /** The wrapped identity was written and reopened successfully. */
    READY("ready\n"),
    /** Confirmed removal began but may need recovery. */
    DELETING("deleting\n"),

    /** Unreadable or tampered metadata: shown for recovery, never opened or overwritten. */
    BROKEN(null),
}

/** One local identity directory and its lifecycle state. */
class Slot(
    /** UUID directory name, or [Slots.LEGACY] for the root identity. */
    val id: String,
    /** Directory containing this slot's wrapped identity. */
    val dir: File,
    /** Current on-disk lifecycle state. */
    var state: SlotState
) {
    /** Wrapped identity key file in [dir]. */
    val keyFile get() = File(dir, IDENTITY_KEY_FILE)
    /** Keystore alias that wraps this slot's key. */
    val alias get() = if (id == Slots.LEGACY) "dyapp-identity-wrap" else "dyapp-identity-wrap:$id"
}

/** Result of opening the active identity or choosing the next app screen. */
sealed class Opened {
    /** No slot yet: create the first identity. */
    object Setup : Opened()

    /** A ready slot whose identity key reopened successfully. */
    class Ready(
        /** Local slot that supplied the key. */
        val slot: Slot,
        /** Verified Peer ID derived from the unwrapped key. */
        val peerId: String
    ) : Opened()

    /** Several slots and no valid pointer. */
    object Picker : Opened()

    /** The slot cannot be used as it is; [peerId] is known when its key opens. */
    class Recovery(
        /** Local slot that needs user action. */
        val slot: Slot,
        /** Stable recovery reason shown by the app. */
        val reason: String,
        /** Peer ID when the wrapped key could still be opened. */
        val peerId: String?
    ) : Opened()
}

/**
 * Local identity slots: `profiles/<uuid>/` directories beside the legacy root key, discovered by a
 * directory scan rather than a registry; see docs/development/android-identity-storage.md. The
 * app has one process, so the instance lock serializes every change.
 */
class Slots(private val root: File, private val keys: SlotKeys) {
    private val profiles = File(root, "profiles")
    private val activeFile = File(root, "active-profile")

    /** All local slots: the legacy one if its key exists, then each UUID directory, by name. */
    fun list(): List<Slot> {
        val slots = mutableListOf<Slot>()
        val legacy = File(root, IDENTITY_KEY_FILE)
        if (exists(legacy)) {
            val state = if (isRegularFile(legacy)) SlotState.READY else SlotState.BROKEN
            slots += Slot(LEGACY, root, state)
        }
        val names = profiles.list() ?: return slots
        names.filter(::isUuid).sorted().mapTo(slots) { readSlot(it, File(profiles, it)) }
        return slots
    }

    // Reserves a fresh slot, written as `pending` before any key exists.
    private fun reserve(): Slot {
        profiles.mkdirs()
        repeat(SLOT_RESERVATION_ATTEMPTS) {
            val id = UUID.randomUUID().toString()
            val slot = Slot(id, File(profiles, id), SlotState.PENDING)
            // Never reuse or delete a key some earlier slot left behind.
            if (keys.exists(slot.alias) || !slot.dir.mkdir()) return@repeat
            Files.write(
                File(slot.dir, SLOT_STATE_FILE).toPath(),
                SlotState.PENDING.marker!!.toByteArray(),
                StandardOpenOption.CREATE_NEW,
                StandardOpenOption.WRITE,
                StandardOpenOption.SYNC,
            )
            return slot
        }
        throw IOException("Could not reserve a new identity slot")
    }

    /** Moves a profile slot to [state]; the legacy slot has no state file. */
    fun setState(slot: Slot, state: SlotState) {
        check(slot.id != LEGACY && slot.state != SlotState.BROKEN) { "Slot state cannot change" }
        writeAtomic(File(slot.dir, SLOT_STATE_FILE), state.marker!!.toByteArray())
        slot.state = state
    }

    /** The slot named by `active-profile`, if the pointer is valid. */
    fun active(): String? {
        val id = runCatching { readMarker(activeFile) }.getOrNull()?.removeSuffix("\n")
        return id?.takeIf { it == LEGACY || isUuid(it) }
    }

    /** Atomically select the local slot [id] for the next app open. */
    fun setActive(id: String) =
        synchronized(this) { writeAtomic(activeFile, "$id\n".toByteArray()) }

    /** Peer ID of the slot's key; null when the file or its alias is confirmed absent. */
    fun load(slot: Slot): String? {
        if (!exists(slot.keyFile) || !keys.exists(slot.alias)) return null
        if (!isRegularFile(slot.keyFile)) throw IOException("Key is not a regular file")
        return keys.peerId(slot.alias, slot.keyFile.readBytes())
    }

    /**
     * Opens the active slot, else the only one. A missing key or incomplete slot is a recovery
     * state, never a reason to create a new identity.
     */
    fun open(): Opened {
        val slots = list()
        val id = active()
        val slot = slots.firstOrNull { it.id == id } ?: slots.singleOrNull()
        return when {
            slot != null -> openSlot(slot, ::load)
            slots.isEmpty() -> Opened.Setup
            else -> Opened.Picker
        }
    }

    /**
     * Creates an identity in a fresh slot and makes it active. The slot is marked ready only
     * after its key reopens with the same Peer ID.
     */
    fun create(): Opened.Ready = synchronized(this) {
        val slot = reserve()
        val (wrapped, peerId) = keys.generate(slot.alias)
        writeAtomic(slot.keyFile, wrapped)
        check(load(slot) == peerId) { "Saved identity does not reopen" }
        setState(slot, SlotState.READY)
        setActive(slot.id)
        Opened.Ready(slot, peerId)
    }

    /**
     * Removes slot [id]. A key that still opens must yield [peerId], the Peer ID the user
     * confirmed. The slot is marked `deleting` before its alias and file go, and its folder
     * goes only once the key is confirmed absent, so any failure keeps a slot a retry can
     * finish. The legacy slot and damaged slots are refused.
     */
    fun remove(id: String, peerId: String?) = synchronized(this) {
        val slot = list().firstOrNull { it.id == id } ?: throw IOException("Unknown identity")
        check(slot.id != LEGACY && slot.state != SlotState.BROKEN) {
            "This identity cannot be removed"
        }
        load(slot)?.let { check(it == peerId) { "The key does not match the confirmed Peer ID" } }
        if (slot.state != SlotState.DELETING) setState(slot, SlotState.DELETING)
        // The alias is the secret: without it any copy of the file is useless.
        if (keys.exists(slot.alias)) keys.delete(slot.alias)
        check(!keys.exists(slot.alias)) { "The key is still present after removal" }
        check(slot.keyFile.delete() || !exists(slot.keyFile)) { "Cannot delete the key file" }
        val names = slot.dir.list() ?: throw IOException("Cannot read the identity folder")
        check(names.all { it in removableSlotFiles }) {
            "The identity folder holds other files"
        }
        // The rename drops the slot from list() at once; a crash after it leaves a hidden folder.
        val removed = File(profiles, ".$id.removed")
        Files.move(slot.dir.toPath(), removed.toPath(), StandardCopyOption.ATOMIC_MOVE)
        check(removed.deleteRecursively()) { "Cannot delete the identity folder" }
        if (active() == id) check(activeFile.delete()) { "Cannot clear the active identity" }
    }

    /** Constants and canonical UUID validation shared by slot operations. */
    companion object {
        /** The virtual slot of the root `identity.key` installation. */
        const val LEGACY = "legacy-primary"

        private val removableSlotFiles = setOf(
            SLOT_STATE_FILE, "state.tmp", "network", "network.tmp", "network-draft", "network-draft.tmp",
            "profile-draft.json", "profile-draft.json.bak", "profile-draft.json.new",
            "client.db", "client.db-wal", "client.db-shm", "client.db-journal"
        )

        private const val SLOT_RESERVATION_ATTEMPTS = 8
        private val UUID_PATTERN = Regex("[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}")

        /** Whether [name] is a canonical lowercase UUID slot directory name. */
        fun isUuid(name: String) = UUID_PATTERN.matches(name)
    }
}
