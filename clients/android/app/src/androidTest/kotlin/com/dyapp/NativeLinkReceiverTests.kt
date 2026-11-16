package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.CoreException
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileLinkApproval
import uniffi.dyapp.MobileLinkCodeStatus
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceiver
import uniffi.dyapp.MobileSlotLayout
import uniffi.dyapp.deviceLinkCodeStatus

/** Real JNI and Android Keystore callbacks; no emulator timing or node hosting. */
class NativeLinkReceiverTests {
    private fun withRegistry(block: (MobileIdentityRegistry) -> Unit) {
        NativeSlotFixture().use { fixture ->
            MobileIdentityRegistry(
                fixture.root.absolutePath,
                MobileSlotLayout.ANDROID,
                NativeSlotKeys,
            ).use(block)
        }
    }

    @Test
    fun preparedOwnerApprovalRejectsProfileAbaBeforeAnyPrivateSnapshot() {
        withRegistry { registry ->
            val fields = listOf("dyapp-link", "1", "01".repeat(32), "02".repeat(32))
            val expired = (fields + "1").joinToString(":")
            val code = (fields + ULong.MAX_VALUE.toString()).joinToString(":")
            assertEquals(MobileLinkCodeStatus.MALFORMED, deviceLinkCodeStatus("bad"))
            assertEquals(MobileLinkCodeStatus.EXPIRED, deviceLinkCodeStatus(expired))
            assertEquals(MobileLinkCodeStatus.VALID, deviceLinkCodeStatus(code))
            val source = registry.create()
            val peer = source.key.use { it.peerId() }
            MobileLinkApproval.prepare(registry, source.slot.id, peer, code).use { approval ->
                assertEquals(code, approval.code())
                assertEquals(ULong.MAX_VALUE, approval.expiresAt())
                val privateStore = java.io.File(source.slot.directory, "client.db")
                assertTrue(!privateStore.exists())
                registry.create().key.close()
                registry.select(source.slot.id).key.close()
                val failure = assertThrows(CoreException.Failed::class.java) { approval.confirmAndSend() }
                assertEquals("Profile view changed", failure.message)
                assertTrue(!privateStore.exists())
                assertEquals(2, registry.list().size)
            }
        }
    }

    @Test
    fun idleReceiverAllowsProfileCreationAndCancellationPreventsAnyInstallation() {
        withRegistry { registry ->
            val source = registry.create()
            source.key.use { owner ->
                MobileLinkReceiver.start(registry, source.slot.id, owner.peerId(), false).use { receiver ->
                    assertTrue(receiver.code().startsWith("dyapp-link:1:"))
                    assertTrue(receiver.expiresAt() > 0uL)
                    assertEquals(MobileLinkPhase.WAITING, receiver.phase())
                    assertNull(receiver.result())
                    registry.create().key.close()
                    assertEquals(MobileLinkPhase.CANCELLED, receiver.cancel())
                    assertEquals(MobileLinkPhase.CANCELLED, receiver.poll())
                    assertNull(receiver.result())
                    assertEquals(2, registry.list().size)
                }
            }
        }
    }

    @Test
    fun missingActiveNetworkAndChangedSourcePreserveTheExistingInventory() {
        withRegistry { registry ->
            val source = registry.create()
            source.key.use { owner ->
                MobileLinkReceiver.start(registry, source.slot.id, owner.peerId(), false).use { receiver ->
                    assertThrows(CoreException.Failed::class.java) { receiver.poll() }
                    assertEquals(MobileLinkPhase.WAITING, receiver.phase())
                    assertNull(receiver.result())
                    assertEquals(1, registry.list().size)
                }
                assertThrows(CoreException.Failed::class.java) {
                    MobileLinkReceiver.start(registry, source.slot.id, "wrong owner", false)
                }
                registry.create().key.close()
                assertThrows(CoreException.Failed::class.java) {
                    MobileLinkReceiver.start(registry, source.slot.id, owner.peerId(), false)
                }
                assertEquals(2, registry.list().size)
            }
        }
    }
}
