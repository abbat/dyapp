package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import java.io.File
import java.util.Locale

internal class OwnerVisualScene(
    val backend: OwnerVisualBackend,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float
) {
    val route = mutableStateOf(SettingsPane.DEVICES)
    val opened = Opened.Ready(
        Slot("visual-only", File(context.cacheDir, "visual-only"), SlotState.READY),
        OWNER_VISUAL_IDENTITY
    )
}

/** One real Compose root; cases retire their actual dialog/worker before the next scene. */
internal class OwnerVisualHost(val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<OwnerVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: OwnerVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide(scene: OwnerVisualScene) {
        compose.runOnIdle { if (current.value === scene) current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: OwnerVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                SettingsFrame(scene.route) { pane ->
                    check(pane == SettingsPane.DEVICES)
                    DeviceDirectoryScreen(scene.backend.directory) { enabled ->
                        LinkApprovalEntry(scene.opened, enabled) { scene.backend.approval }
                    }
                }
            }
        }
    }
}

internal fun ownerVisualContext(language: String, scale: Float): Context {
    val base = InstrumentationRegistry.getInstrumentation().targetContext
    val config = Configuration(base.resources.configuration).apply {
        setLocale(Locale.forLanguageTag(language))
        fontScale = scale
    }
    return base.createConfigurationContext(config)
}
