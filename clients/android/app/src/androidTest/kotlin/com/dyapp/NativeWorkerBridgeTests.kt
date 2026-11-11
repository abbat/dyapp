package com.dyapp

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class NativeWorkerBridgeTests {
    @Test
    fun chatBootstrapAndRemovalShareActualNativeOwnershipAndWrappedKeys() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            RustBridge.withChats(ready) { assertTrue(it.chatRows().isEmpty()) }
            assertTrue(File(ready.slot.dir, "device.key").length() > 32)
            assertTrue(File(ready.slot.dir, "chat-state.key").length() > 32)
            fixture.slots.remove(ready.slot.id, ready.peerId)
            assertFalse(ready.slot.dir.exists())
        }
    }

    @Test
    fun anIdleSearchHandleCannotResurrectARemovedSlot() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            ready.profileSearcher().use { searcher ->
                fixture.slots.remove(ready.slot.id, ready.peerId)
                assertTrue(runCatching { searcher.contactNamesSlot(emptyList()) }.isFailure)
                assertFalse(ready.slot.dir.exists())
            }
        }
    }

    @Test
    fun anIdleProofHandleCannotResurrectARemovedSlot() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            ready.profileSolver().use { solve ->
                fixture.slots.remove(ready.slot.id, ready.peerId)
                assertTrue(runCatching { solve.runStep(true) }.isFailure)
                assertFalse(ready.slot.dir.exists())
            }
        }
    }

    @Test
    fun anIdleNetworkHandleCannotResurrectARemovedSlot() {
        NativeSlotFixture().use { fixture ->
            val ready = fixture.slots.create()
            ready.connectionChecker().use { checker ->
                fixture.slots.remove(ready.slot.id, ready.peerId)
                assertTrue(runCatching { checker.activate() }.isFailure)
                assertFalse(ready.slot.dir.exists())
            }
        }
    }
}
