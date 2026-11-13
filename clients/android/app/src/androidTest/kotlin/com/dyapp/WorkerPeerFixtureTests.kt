package com.dyapp

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import java.io.File
import java.io.FileNotFoundException

class WorkerPeerFixtureTests {
    private fun checkpoint(fixture: NativeSlotFixture) =
        File(fixture.root.parentFile, "${fixture.root.name}.worker.json")

    @Test
    fun reopeningAndCleanupPreserveBothExistingIdentitiesAndThePreviousSelection() {
        NativeSlotFixture().use { fixture ->
            val previous = fixture.slots.create()
            val second = fixture.slots.create()
            fixture.slots.choose(previous.slot.id)
            val inventory = fixture.slots.list().map { it.id }.sorted()
            val checkpoint = checkpoint(fixture)
            val worker = WorkerPeerFixture(fixture.root, checkpoint)
            val ready = worker.prepare()
            val reopened = WorkerPeerFixture(fixture.root, checkpoint).reopen()
            assertEquals(ready.slot.id, reopened.slot.id)
            assertEquals(ready.peerId, reopened.peerId)
            assertEquals(ready.peerId, RustBridge.withKey(reopened) { _, key -> key.peerId() })
            assertTrue(RustBridge.withChats(reopened) { it.chatRows() }.isEmpty())
            assertThrows(IllegalStateException::class.java) { worker.prepare() }
            WorkerPeerFixture(fixture.root, checkpoint).cleanup()
            assertEquals(previous.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            assertEquals(inventory, fixture.slots.list().map { it.id }.sorted())
            assertEquals(second.peerId, (fixture.slots.inspect(second.slot.id) as Opened.Ready).peerId)
            worker.cleanup()
            assertThrows(IllegalStateException::class.java) { worker.reopen() }
        }
    }

    @Test
    fun aFreshRegistryReturnsToSetupAndAllowsAnExplicitNewFixture() {
        NativeSlotFixture().use { fixture ->
            val worker = WorkerPeerFixture(fixture.root, checkpoint(fixture))
            worker.prepare()
            worker.cleanup()
            assertEquals(Opened.Setup, fixture.slots.open())
            assertTrue(fixture.slots.list().isEmpty())
            worker.prepare()
            worker.cleanup()
            assertEquals(Opened.Setup, fixture.slots.open())
        }
    }

    @Test
    fun anExistingPickerReturnsWithoutSelectingOneOfItsIdentities() {
        NativeSlotFixture().use { fixture ->
            fixture.slots.create()
            fixture.slots.create()
            File(fixture.root, "active-profile").writeText("none\n")
            assertEquals(Opened.Picker, fixture.slots.open())
            val inventory = fixture.slots.list().map { it.id }.sorted()
            val worker = WorkerPeerFixture(fixture.root, checkpoint(fixture))
            worker.prepare()
            worker.cleanup()
            assertEquals(Opened.Picker, fixture.slots.open())
            assertEquals(inventory, fixture.slots.list().map { it.id }.sorted())
        }
    }

    @Test
    fun recoveryIsRefusedBeforeAnyIdentityOrCheckpointIsCreated() {
        NativeSlotFixture().use { fixture ->
            check(fixture.root.mkdirs())
            val pointer = File(fixture.root, "active-profile").apply { writeText("../escape\n") }
            val checkpoint = checkpoint(fixture)
            val worker = WorkerPeerFixture(fixture.root, checkpoint)
            assertThrows(IllegalStateException::class.java) { worker.prepare() }
            assertTrue(fixture.slots.open() is Opened.Recovery)
            assertEquals("../escape\n", pointer.readText())
            assertTrue(fixture.slots.list().isEmpty())
            assertFalse(checkpoint.exists())
        }
    }

    @Test
    fun reopenNeverReplacesAnInterveningSelection() {
        NativeSlotFixture().use { fixture ->
            val previous = fixture.slots.create()
            val worker = WorkerPeerFixture(fixture.root, checkpoint(fixture))
            worker.prepare()
            fixture.slots.choose(previous.slot.id)
            assertThrows(IllegalStateException::class.java) { worker.reopen() }
            assertEquals(previous.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            worker.cleanup()
            assertEquals(previous.peerId, (fixture.slots.open() as Opened.Ready).peerId)
        }
    }

    @Test
    fun cleanupFailureRestoresSelectionAndRetryResumesAfterNativeRemoval() {
        NativeSlotFixture().use { fixture ->
            val previous = fixture.slots.create()
            val worker = WorkerPeerFixture(fixture.root, checkpoint(fixture))
            val ready = worker.prepare()
            val foreign = File(ready.slot.dir, "notes.txt").apply { writeText("keep") }
            assertThrows(CoreException::class.java) { worker.cleanup() }
            assertEquals(previous.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            assertEquals(ready.peerId, (fixture.slots.inspect(ready.slot.id) as Opened.Ready).peerId)
            assertTrue(foreign.renameTo(File(fixture.root.parentFile, "${fixture.root.name}.foreign-note")))
            fixture.slots.remove(ready.slot.id, ready.peerId)
            WorkerPeerFixture(fixture.root, checkpoint(fixture)).cleanup()
            assertEquals(listOf(previous.slot.id), fixture.slots.list().map { it.id })
            assertEquals(previous.peerId, (fixture.slots.open() as Opened.Ready).peerId)
        }
    }

    @Test
    fun aCheckpointCannotRepurposeAnEarlierIdentityAsTheWorkerFixture() {
        NativeSlotFixture().use { fixture ->
            val previous = fixture.slots.create()
            val checkpoint = checkpoint(fixture)
            val worker = WorkerPeerFixture(fixture.root, checkpoint)
            val ready = worker.prepare()
            val original = checkpoint.readText()
            val forged = JSONObject(original).put(
                "fixture",
                JSONObject().put("id", previous.slot.id).put("peer", previous.peerId)
            )
            checkpoint.writeText(forged.toString())
            assertThrows(IllegalStateException::class.java) { worker.reopen() }
            assertThrows(IllegalStateException::class.java) { worker.cleanup() }
            assertEquals(previous.peerId, (fixture.slots.inspect(previous.slot.id) as Opened.Ready).peerId)
            assertEquals(ready.peerId, (fixture.slots.open() as Opened.Ready).peerId)
            checkpoint.writeText(original)
            worker.cleanup()
            assertEquals(listOf(previous.slot.id), fixture.slots.list().map { it.id })
        }
    }

    @Test
    fun missingOrInterruptedCheckpointNeverCreatesAReplacement() {
        NativeSlotFixture().use { fixture ->
            val checkpoint = checkpoint(fixture)
            val worker = WorkerPeerFixture(fixture.root, checkpoint)
            assertThrows(FileNotFoundException::class.java) { worker.reopen() }
            checkpoint.writeText("""{"stage":"preparing"}""")
            assertThrows(IllegalStateException::class.java) { worker.prepare() }
            assertThrows(IllegalStateException::class.java) { worker.cleanup() }
            assertEquals(Opened.Setup, fixture.slots.open())
            assertTrue(fixture.slots.list().isEmpty())
        }
    }
}
