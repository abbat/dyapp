package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileDeviceDirectory
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileLinkApproval
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileLinkReceiver
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.MobileSlotSelection

/** Both phones are clients of the existing CI node; neither hosts a node. */
class NativeLinkJourneyTests {
    @Test
    fun confirmedOwnerTransfersPrivateStateAndReceiverCompletesEnrollment() {
        workerTestPolicy()
        withPhones { owner, recipient ->
            val original = createReady(owner)
            val destination = createReady(recipient)
            publishWorkerPeer(original)
            activateWorkerPeer(destination)
            val contact = IdentityKey.generate().use { it.peerId() }
            val note = "private note transferred through the authenticated link"
            RustBridge.withChats(original) {
                it.setContactNote(contact, note)
                it.setFavorite(contact, true)
            }
            transfer(owner, original, recipient, destination).let { installed ->
                assertEquals(original.peerId, installed.peerId)
                assertNotEquals(destination.slot.id, installed.slot.id)
                assertEquals(2, recipient.list().size)
                // Normal session bootstrap preserves the received device before pending-state import.
                assertNativePrivateContact(installed, contact, "", saved = false)
                completeEnrollment(installed)
                assertNativePrivateContact(installed, contact, note, saved = true)
                verifyNativeOwnPrivateSync(original, installed, contact)
                verifyDirectoryAndRevocation(original, installed, destination)
            }
        }
    }

    private fun withPhones(block: (MobileIdentityRegistry, MobileIdentityRegistry) -> Unit) {
        withPhone { owner -> withPhone { recipient -> block(owner, recipient) } }
    }

    private fun withPhone(block: (MobileIdentityRegistry) -> Unit) {
        NativeSlotFixture().use { fixture ->
            MobileIdentityRegistry(fixture.root.absolutePath, MobileSlotLayout.ANDROID, NativeSlotKeys).use(block)
        }
    }

    private fun createReady(registry: MobileIdentityRegistry): Opened.Ready = ready(registry, registry.create())

    private fun ready(registry: MobileIdentityRegistry, selection: MobileSlotSelection): Opened.Ready =
        selection.key.use { Opened.Ready(selection.slot.view(registry), it.peerId()) }

    private fun transfer(
        owner: MobileIdentityRegistry,
        original: Opened.Ready,
        recipient: MobileIdentityRegistry,
        destination: Opened.Ready
    ): Opened.Ready =
        MobileLinkReceiver.start(recipient, destination.slot.id, destination.peerId, false).use { receiver ->
            MobileLinkApproval.prepare(owner, original.slot.id, original.peerId, receiver.code()).use { approval ->
                approval.confirmAndSend()
            }
            PeerTransition("authenticated link installation").await(
                ready = { receiver.phase() == MobileLinkPhase.COMPLETED },
                advance = {
                    val phase = receiver.poll()
                    check(phase == MobileLinkPhase.WAITING || phase == MobileLinkPhase.COMPLETED) {
                        "Link stopped at $phase: ${receiver.result()}"
                    }
                }
            )
            val receipt = receiver.result()
            check(receipt is MobileLinkReceipt.Installed) { "Missing installed receipt: $receipt" }
            ready(recipient, receipt.selection)
        }

    private fun directory(opened: Opened.Ready) = MobileDeviceDirectory(
        opened.slot.owner(),
        opened.slot.id,
        opened.peerId,
        PlatformKeyStore.existing(identityAlias(opened.slot.id))
    )

    private fun verifyDirectoryAndRevocation(
        original: Opened.Ready,
        installed: Opened.Ready,
        destination: Opened.Ready
    ) {
        directory(original).use { owner ->
            val before = owner.devices(true)
            assertEquals(2, before.size)
            val originalKey = before.single { it.current }.key
            directory(installed).use { recipient ->
                val linked = recipient.devices(true)
                assertEquals(2, linked.size)
                val linkedKey = linked.single { it.current }.key
                assertFalse(originalKey.contentEquals(linkedKey))
                assertTrue(before.any { it.key.contentEquals(linkedKey) })
                assertNull(owner.revokedDevice())
                assertNull(completeRevokedDevice(installed))
                owner.revoke(linkedKey)
                val remaining = owner.devices(true)
                assertEquals(1, remaining.size)
                assertTrue(remaining.single().current)
                assertTrue(remaining.single().key.contentEquals(originalKey))
                val received = recipient.devices(true)
                assertEquals(1, received.size)
                assertFalse(received.single().current)
                assertTrue(received.single().key.contentEquals(originalKey))
                verifyRevokedCleanup(installed, destination)
            }
        }
    }

    private fun verifyRevokedCleanup(
        installed: Opened.Ready,
        destination: Opened.Ready
    ) {
        val registry = installed.slot.owner()
        registry.select(destination.slot.id).key.use { assertEquals(destination.peerId, it.peerId()) }
        val completed = checkNotNull(completeRevokedDevice(installed)) { "Missing verified revocation" }
        assertNull(completed.getOrThrow())
        assertNull(completeRevokedDevice(installed))
        assertEquals(destination.slot.id, registry.list().single().id)
        registry.select(destination.slot.id).key.use { assertEquals(destination.peerId, it.peerId()) }
    }

    private fun completeEnrollment(installed: Opened.Ready) {
        linkEnrollmentReview(installed.slot.owner(), installed.slot.id, installed.peerId).use { review ->
            assertTrue(review.resume())
            // A second completion proves that the original pending-link row was cleared.
            assertFalse(review.resume())
        }
    }
}
