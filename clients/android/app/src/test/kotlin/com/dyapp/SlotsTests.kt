package com.dyapp

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Wrapping keys in a map; a blob "unwraps" to its Peer ID only under the alias that made it. */
private class FakeKeys : SlotKeys {
    val aliases = mutableMapOf<String, Int>()
    var deleteWorks = true
    private var next = 0

    override fun exists(alias: String) = alias in aliases

    override fun generate(alias: String): Pair<ByteArray, String> {
        aliases[alias] = ++next
        return Pair("$alias|peer-$next".toByteArray(), "peer-$next")
    }

    override fun peerId(alias: String, wrapped: ByteArray): String {
        val (owner, peer) = String(wrapped).split("|")
        check(owner == alias) { "wrong key" }
        return peer
    }

    override fun delete(alias: String) {
        if (deleteWorks) aliases.remove(alias)
    }
}

class SlotsTests {
    private val root = Files.createTempDirectory("dyapp-slots-test").toFile()
    private val keys = FakeKeys()
    private val slots = Slots(root, keys)

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    private fun state() = when (val opened = slots.open()) {
        Opened.Setup -> "setup"
        Opened.Picker -> "picker"
        is Opened.Ready -> "ready ${opened.slot.id} ${opened.peerId}"
        is Opened.Recovery -> "${opened.reason} ${opened.slot.id} ${opened.peerId}"
    }

    @Test
    fun createSwitchAndReopenKeepDistinctIdentities() {
        assertEquals("setup", state())
        val first = slots.create()
        assertTrue(Slots.isUuid(first.slot.id))
        assertEquals("dyapp-identity-wrap:${first.slot.id}", first.slot.alias)
        assertEquals("ready ${first.slot.id} ${first.peerId}", state())
        val second = slots.create()
        assertTrue(first.peerId != second.peerId)
        assertEquals("ready ${second.slot.id} ${second.peerId}", state())
        File(root, "active-profile").delete()
        assertEquals("picker", state())
        slots.setActive(first.slot.id)
        assertEquals("ready ${first.slot.id} ${first.peerId}", state())
        // A corrupt pointer falls back to the picker.
        File(root, "active-profile").writeText("../escape\n")
        assertNull(slots.active())
        assertEquals("picker", state())
    }

    @Test
    fun missingKeysAndIncompleteSlotsAreRecoveryNotNewKeys() {
        val ready = slots.create()
        keys.aliases.remove(ready.slot.alias)
        assertEquals("missing ${ready.slot.id} null", state())
        // A pending slot from a crash after the key file is written shows its Peer ID.
        val slot = ready.slot
        File(slot.dir, "state").writeText("pending\n")
        keys.generate(slot.alias).let { slot.keyFile.writeBytes(it.first) }
        assertEquals("incomplete ${slot.id} ${slots.load(slot)}", state())
        // A key that does not unwrap is unavailable, never replaced.
        slot.keyFile.writeBytes("other|peer-0".toByteArray())
        assertEquals("unavailable ${slot.id} null", state())
        File(slot.dir, "state").writeText("exploded\n")
        assertEquals("damaged ${slot.id} null", state())
        assertEquals(1, slots.list().size)
    }

    @Test
    fun legacyKeyOpensInPlaceAndIsNeverRemoved() {
        val (wrapped, peer) = keys.generate("dyapp-identity-wrap")
        File(root, "identity.key").writeBytes(wrapped)
        assertEquals("ready ${Slots.LEGACY} $peer", state())
        val added = slots.create()
        assertEquals(listOf(Slots.LEGACY, added.slot.id), slots.list().map { it.id })
        assertTrue(runCatching { slots.remove(Slots.LEGACY, peer) }.isFailure)
        assertTrue(File(root, "identity.key").exists())
    }

    @Test
    fun removeErasesOnlyTheConfirmedKey() {
        val first = slots.create()
        val second = slots.create()
        val id = second.slot.id
        // Another identity's Peer ID or none at all leaves the slot untouched.
        assertTrue(runCatching { slots.remove(id, first.peerId) }.isFailure)
        assertTrue(runCatching { slots.remove(id, null) }.isFailure)
        assertEquals(SlotState.READY, slots.list().first { it.id == id }.state)
        // A delete that leaves the alias keeps the slot, now deleting, for a retry.
        keys.deleteWorks = false
        assertTrue(runCatching { slots.remove(id, second.peerId) }.isFailure)
        assertEquals("deleting $id ${second.peerId}", state())
        keys.deleteWorks = true
        // Network settings go with the slot.
        File(second.slot.dir, "network").writeText("dyapp-network 1\nname \n")
        slots.remove(id, second.peerId)
        assertTrue(slots.list().none { it.id == id })
        assertNull(slots.active())
        assertTrue(slots.list().any { it.id == first.slot.id })
        // Foreign files keep the slot.
        File(first.slot.dir, "notes.txt").writeText("mine")
        assertTrue(runCatching { slots.remove(first.slot.id, first.peerId) }.isFailure)
        assertEquals(listOf("notes.txt", "state"), first.slot.dir.list()!!.sorted())
    }
}
