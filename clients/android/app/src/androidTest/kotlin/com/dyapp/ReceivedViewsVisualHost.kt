package com.dyapp

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.CompletableDeferred
import uniffi.dyapp.ReceivedProfileView

internal class ReceivedViewsVisualScene(
    val state: ReceivedViewsVisualState,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float
) {
    val route = mutableStateOf(SettingsPane.VIEWS)
    val metadata = NativeScreenVisualScene("settings-views", state.screen, context, dark, fontScale)
    private var reads = 0

    suspend fun read(): List<ReceivedProfileView> {
        reads += 1
        return when {
            state == ReceivedViewsVisualState.LOADING -> CompletableDeferred<List<ReceivedProfileView>>().await()
            state == ReceivedViewsVisualState.ERROR -> error("controlled initial read failure")
            state == ReceivedViewsVisualState.STALE && reads > 1 -> error("controlled refresh failure")
            state == ReceivedViewsVisualState.EMPTY -> emptyList()
            else -> listOf(
                ReceivedProfileView("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "2024-02-29")
            )
        }
    }
}

/** Production Settings frame and received collection; controlled reads never fetch profiles or send views. */
internal class ReceivedViewsVisualHost(val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<ReceivedViewsVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: ReceivedViewsVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: ReceivedViewsVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                SettingsFrame(scene.route) {
                    ReceivedViewsCollection(scene, scene::read) {}
                }
            }
        }
    }
}
