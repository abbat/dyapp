package com.dyapp

import android.content.ClipboardManager
import androidx.compose.ui.test.assertDoesNotExist
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals

private const val DEVICE_UI_STEP_MS = 180_000L

/** Real node/keystore/JNI write through the production menu and screen, without another link setup. */
internal class NativeDeviceScreenJourney(private val host: NativeLinkScreenHost) {
    private val compose = host.compose

    fun revoke(original: Opened.Ready, key: ByteArray) {
        val session = DeviceDirectorySession { nativeDeviceDirectoryCalls(original) }
        val scene = NativeLinkScreenScene(original, session)
        try {
            host.show(scene)
            host.openMenu(original)
            val listing = host.verifiedDirectory(scene)
            val current = listing.rows.single { it.current }
            val row = listing.rows.single { it.key().contentEquals(key) }
            assertCurrentReadOnly(current.identifier)
            openConfirmation(row.identifier)
            compose.onNodeWithTag("disconnect-device").assertTextEquals(row.identifier)
            copyTarget(row.identifier)
            compose.onNodeWithTag("disconnect-confirm").performClick()
            val completed = awaitRevoke(session)
            completed.result.getOrThrow()
            assertArrayEquals(key, completed.target.key())
            compose.awaitNode(hasTestTag("devices-completion") and hasText(text(R.string.devices_gone)))
            compose.onNodeWithTag("devices-completion-target").assertTextEquals(row.identifier)
        } finally {
            close(session, scene)
        }
    }

    private fun assertCurrentReadOnly(identifier: String) {
        compose.awaitNode(hasTestTag("device-row-" + identifier))
        compose.onNode(
            hasText(text(R.string.devices_disconnect)) and
                hasAnyAncestor(hasTestTag("device-row-$identifier"))
        ).assertDoesNotExist()
    }

    private fun openConfirmation(identifier: String) {
        compose.awaitNode(
            hasText(text(R.string.devices_disconnect)) and
                hasAnyAncestor(hasTestTag("device-row-$identifier"))
        ).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("disconnect-dialog"))
        compose.awaitNode(hasTestTag("disconnect-cancel") and isFocused())
    }

    private fun awaitRevoke(session: DeviceDirectorySession): DeviceDisconnectCompletion = runBlocking {
        val view = withTimeout(DEVICE_UI_STEP_MS) {
            session.state.first { !it.busy && it.completion != null }
        }
        checkNotNull(view.completion)
    }

    private fun close(session: DeviceDirectorySession, scene: NativeLinkScreenScene) {
        try {
            host.hide(scene)
        } finally {
            runBlocking {
                withTimeout(DEVICE_UI_STEP_MS) { session.retire().await().getOrThrow() }
            }
        }
    }

    private fun copyTarget(identifier: String) {
        compose.awaitNode(hasTestTag("copy-disconnect-device")).performScrollTo().performClick()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        assertEquals(identifier, clipboard.primaryClip?.getItemAt(0)?.text?.toString())
    }

    private fun text(id: Int): String = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)
}
