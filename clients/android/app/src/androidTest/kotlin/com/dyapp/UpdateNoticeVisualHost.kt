package com.dyapp

import android.content.Context
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

internal enum class UpdateNoticeVisualState(val screen: String, val permission: Boolean = false) {
    PACKAGE("update-notice-package"),
    SOURCE("update-notice-source"),
    SAVING("update-notice-saving"),
    WRITE_ERROR("update-notice-write-error"),
    OPEN_ERROR("update-notice-open-error"),
    PERMISSION_ALLOWED("update-notice-permission-allowed", true),
    PERMISSION_BLOCKED("update-notice-permission-blocked", true),
    PERMISSION_PENDING("update-notice-permission-pending", true),
    PERMISSION_FAILED("update-notice-permission-failed", true),
    PERMISSION_UNKNOWN("update-notice-permission-unknown", true)
}

internal class UpdateNoticeVisualScene(
    val state: UpdateNoticeVisualState,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float,
) {
    private val root = if (state.permission) "settings-about" else "update-notice"
    val metadata = NativeScreenVisualScene(root, state.screen, context, dark, fontScale)
    val route = mutableStateOf(SettingsPane.ABOUT)
    val registry = UpdatePermissionRegistry(state == UpdateNoticeVisualState.PERMISSION_FAILED)

    fun readPermission(): Boolean {
        check(state != UpdateNoticeVisualState.PERMISSION_UNKNOWN) { "Controlled permission query failure" }
        return state == UpdateNoticeVisualState.PERMISSION_ALLOWED
    }

    fun presentation(): UpdateNoticeUiState {
        val method = if (state == UpdateNoticeVisualState.SOURCE) {
            MobileUpdateDelivery.BUILD_FROM_SOURCE
        } else {
            MobileUpdateDelivery.PACKAGE
        }
        val offer = MobileUpdateOffer("0.0.2", method, "https://github.com/abbat/dyapp/releases/tag/v0.0.2")
        val snapshot = MobileUpdateState(
            installedVersion = "0.0.1",
            interval = MobileUpdateInterval.DAY,
            lastAttempt = 1uL,
            lastSuccess = 1uL,
            inFlight = false,
            availability = MobileUpdateAvailability.AVAILABLE,
            offer = offer,
            notice = offer,
        )
        return UpdateNoticeUiState(
            snapshot,
            pending = state == UpdateNoticeVisualState.SAVING,
            failed = state == UpdateNoticeVisualState.WRITE_ERROR,
        )
    }
}

/** Production dialog with controlled core records; no HTTP, persistence or real browser launch. */
internal class UpdateNoticeVisualHost(private val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<UpdateNoticeVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: UpdateNoticeVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun PermissionScene(scene: UpdateNoticeVisualScene) {
        CompositionLocalProvider(LocalActivityResultRegistryOwner provides scene.registry) {
            SettingsFrame(scene.route) {
                UpdateMetadataCard(scene, readPermission = { scene.readPermission() }) {
                    checkNotNull(scene.presentation().snapshot)
                }
            }
        }
    }

    @Composable
    private fun Scene(scene: UpdateNoticeVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                if (scene.state.permission) {
                    PermissionScene(scene)
                } else {
                    UpdateNoticeDialog(scene.presentation(), {}) { error("Controlled unavailable browser") }
                }
            }
        }
    }
}
