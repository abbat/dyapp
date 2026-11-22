package com.dyapp

import android.content.ClipboardManager
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertDoesNotExist
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import uniffi.dyapp.MobileDeviceReadState

private const val DEVICE_UI_STEP_MS = 180_000L

/** Real node/keystore/JNI write through the production menu and screen, without another link setup. */
internal class NativeDeviceScreenJourney(private val compose: ComposeContentTestRule) {
    private val shown = mutableStateOf(true)

    fun revoke(original: Opened.Ready, key: ByteArray) {
        val session = DeviceDirectorySession { nativeDeviceDirectoryCalls(original) }
        try {
            show(original, session)
            openMenu(original)
            val listing = awaitDirectory(session)
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
            close(session)
        }
    }

    private fun show(original: Opened.Ready, session: DeviceDirectorySession) {
        val route = mutableStateOf(SettingsPane.ACCOUNT)
        compose.setContent {
            MaterialTheme {
                if (shown.value) {
                    SettingsFrame(route) { pane ->
                        when (pane) {
                            SettingsPane.ACCOUNT -> DeviceDirectoryMenu(original, route)
                            SettingsPane.DEVICES -> DeviceDirectoryScreen(session)
                            else -> error("Unexpected device-screen test route")
                        }
                    }
                }
            }
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

    private fun awaitDirectory(session: DeviceDirectorySession): DeviceDirectoryListing = runBlocking {
        val view = withTimeout(DEVICE_UI_STEP_MS) {
            session.state.first {
                !it.busy && (
                    it.failure != null || it.listing?.state?.let { state ->
                        state != MobileDeviceReadState.CACHED
                    } == true
                    )
            }
        }
        view.failure?.let { throw it }
        val listing = checkNotNull(view.listing)
        assertEquals(MobileDeviceReadState.VERIFIED, listing.state)
        listing
    }

    private fun awaitRevoke(session: DeviceDirectorySession): DeviceDisconnectCompletion = runBlocking {
        val view = withTimeout(DEVICE_UI_STEP_MS) {
            session.state.first { !it.busy && it.completion != null }
        }
        checkNotNull(view.completion)
    }

    private fun close(session: DeviceDirectorySession) {
        try {
            compose.runOnIdle { shown.value = false }
            compose.waitForIdle()
        } finally {
            runBlocking {
                withTimeout(DEVICE_UI_STEP_MS) { session.retire().await().getOrThrow() }
            }
        }
    }

    private fun openMenu(original: Opened.Ready) {
        val calls = nativeDeviceDirectoryCalls(original)
        val count = try {
            calls.read(false).devices.size
        } finally {
            calls.close()
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val expected = context.getString(R.string.devices_count, count)
        compose.awaitNode(hasTestTag("settings-devices-open") and hasText(expected)).performClick()
    }

    private fun copyTarget(identifier: String) {
        compose.awaitNode(hasTestTag("copy-disconnect-device")).performScrollTo().performClick()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        assertEquals(identifier, clipboard.primaryClip?.getItemAt(0)?.text?.toString())
    }

    private fun text(id: Int): String = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)
}
