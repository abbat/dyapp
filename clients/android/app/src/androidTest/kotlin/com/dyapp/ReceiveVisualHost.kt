package com.dyapp

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import com.dyapp.ui.theme.DYAppTheme

internal class ReceiveVisualScene(
    val state: ReceiveVisualState,
    val fixture: ReceiveVisualFixture,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float
) {
    val route = mutableStateOf(SettingsPane.RECEIVE)
    val snapshot = fixture.snapshot(state)
}

/** No worker, network or enrollment action is run by controlled rendering. */
internal class ReceiveVisualHost(val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<ReceiveVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: ReceiveVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: ReceiveVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                SettingsFrame(scene.route) {
                    if (scene.state.intro) {
                        LinkReceiveIntro(scene.fixture.actions, scene.state == ReceiveVisualState.WIZARD_INTRO)
                    } else {
                        LinkReceiveContent(
                            scene.fixture.sourcePeer,
                            scene.snapshot,
                            scene.fixture.status(scene.state),
                            scene.fixture.actions
                        )
                    }
                }
            }
        }
    }
}
