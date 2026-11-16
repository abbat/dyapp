package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileLinkApproval
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileLinkReceiver
import uniffi.dyapp.MobileLinkRecovery
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
            RustBridge.withChats(original) { it.setContactNote(contact, note) }
            transfer(owner, original, recipient, destination).let { installed ->
                assertEquals(original.peerId, installed.peerId)
                assertNotEquals(destination.slot.id, installed.slot.id)
                assertEquals(2, recipient.list().size)
                completeEnrollment(installed)
                assertEquals(note, RustBridge.withChats(installed) { it.contactNote(contact) })
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

    private fun completeEnrollment(installed: Opened.Ready) {
        MobileLinkRecovery(
            installed.slot.owner(),
            installed.slot.id,
            installed.peerId,
            PlatformKeyStore.existing(identityAlias(installed.slot.id))
        ).use { recovery ->
            recovery.prepare().use { review ->
                assertTrue(review.resumeLink())
                // A second completion proves that the original pending-link row was cleared.
                assertFalse(review.resumeLink())
            }
        }
    }
}
