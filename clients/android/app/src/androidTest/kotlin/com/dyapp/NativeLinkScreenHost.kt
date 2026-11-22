package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import uniffi.dyapp.MobileDeviceReadState

internal const val NATIVE_LINK_UI_STEP_MS = 180_000L

/** One Compose root per real wire test; each phase receives a fresh reference-owned route and scene. */
internal class NativeLinkScreenHost(val compose: ComposeContentTestRule) {
    private val shown = mutableStateOf<NativeLinkScreenScene?>(null)

    init {
        compose.setContent {
            MaterialTheme {
                shown.value?.let { scene -> key(scene) { Scene(scene) } }
            }
        }
    }

    fun show(scene: NativeLinkScreenScene) {
        compose.runOnIdle { shown.value = scene }
        compose.waitForIdle()
    }

    fun hide(scene: NativeLinkScreenScene) {
        compose.runOnIdle { if (shown.value === scene) shown.value = null }
        compose.waitForIdle()
    }

    fun openMenu(original: Opened.Ready) {
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

    fun verifiedDirectory(scene: NativeLinkScreenScene): DeviceDirectoryListing = runBlocking {
        val view = withTimeout(NATIVE_LINK_UI_STEP_MS) {
            scene.directory.state.first {
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

    @Composable
    private fun Scene(scene: NativeLinkScreenScene) {
        SettingsFrame(scene.route) { pane ->
            when (pane) {
                SettingsPane.ACCOUNT -> DeviceDirectoryMenu(scene.opened, scene.route)
                SettingsPane.DEVICES -> DeviceDirectoryScreen(scene.directory) { enabled ->
                    LinkApprovalEntry(scene.opened, enabled) {
                        scene.approval ?: nativeLinkApprovalSession(scene.opened)
                    }
                }
                else -> error("Unexpected device-screen test route")
            }
        }
    }
}

internal class NativeLinkScreenScene(
    val opened: Opened.Ready,
    val directory: DeviceDirectorySession,
    val approval: LinkApprovalSession? = null
) {
    val route = mutableStateOf(SettingsPane.ACCOUNT)
}
