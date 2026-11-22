package com.dyapp

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlotLayout

/** Complements the unchanged owner-screen journey with an actual receiving-screen/JNI path. */
class NativeReceiveUiJourneyTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun receivingControlsInstallAndEnrollWithoutReplacingTheExistingProfile() {
        workerTestPolicy()
        withPhone { owner ->
            withPhone { recipient ->
                val original = createReady(owner)
                val destination = createReady(recipient)
                publishWorkerPeer(original)
                activateWorkerPeer(destination)
                val contact = IdentityKey.generate().use { it.peerId() }
                val note = "private state imported through receiving controls"
                RustBridge.withChats(original) {
                    it.setContactNote(contact, note)
                    it.setFavorite(contact, true)
                }
                warmOwner(original)
                receive(original, destination, contact, note)
                assertEquals(2, recipient.list().size)
                recipient.select(destination.slot.id).key.use { assertEquals(destination.peerId, it.peerId()) }
            }
        }
    }

    private fun receive(original: Opened.Ready, destination: Opened.Ready, contact: String, note: String) {
        NativeReceiveUiFixture(compose, destination).use { screen ->
            val display = screen.start()
            compose.onNodeWithTag("link-receive-source").assertTextEquals(destination.peerId)
            compose.onNodeWithTag("link-receive-code").performScrollTo().assertTextEquals(display.code)
            compose.onNodeWithTag("link-receive-qr").performScrollTo().assertIsDisplayed()
            approve(original, display.code)
            val receipt = screen.installed()
            val selected = receipt.selection
            val installed = Opened.Ready(selected.slot.view(destination.slot.owner()), selected.key.peerId())
            assertEquals(original.peerId, installed.peerId)
            assertNotEquals(destination.slot.id, installed.slot.id)
            screen.reopenWithRetiredActions()
            assertNativePrivateContact(installed, contact, "", saved = false)
            screen.finishEnrollment()
            assertNativePrivateContact(installed, contact, note, saved = true)
            assertFinished(installed)
        }
    }

    private fun approve(original: Opened.Ready, code: String) = runBlocking {
        val approval = nativeLinkApprovalSession(original)
        try {
            withTimeout(NATIVE_LINK_UI_STEP_MS) { checkNotNull(approval.prepare(code)).await() }
            val prepared = approval.state.value
            prepared.failure?.let { throw it }
            val opening = checkNotNull(prepared.opening)
            assertEquals(code, opening.code)
            withTimeout(NATIVE_LINK_UI_STEP_MS) { checkNotNull(approval.confirm(opening)).await() }
            approval.state.value.failure?.let { throw it }
            checkNotNull(approval.state.value.delivery).result.getOrThrow()
        } finally {
            withTimeout(NATIVE_LINK_UI_STEP_MS) { approval.retire().await().getOrThrow() }
        }
    }

    private fun assertFinished(installed: Opened.Ready) {
        linkEnrollmentReview(installed.slot.owner(), installed.slot.id, installed.peerId).use { review ->
            assertFalse(review.resume())
        }
    }

    private fun warmOwner(original: Opened.Ready) {
        val calls = nativeDeviceDirectoryCalls(original)
        try {
            assertTrue(calls.read(true).devices.isNotEmpty())
        } finally {
            calls.close()
        }
    }

    private fun createReady(registry: MobileIdentityRegistry): Opened.Ready {
        val selected = registry.create()
        return selected.key.use { Opened.Ready(selected.slot.view(registry), it.peerId()) }
    }

    private fun withPhone(block: (MobileIdentityRegistry) -> Unit) {
        NativeSlotFixture().use { fixture ->
            MobileIdentityRegistry(fixture.root.absolutePath, MobileSlotLayout.ANDROID, NativeSlotKeys).use(block)
        }
    }
}
