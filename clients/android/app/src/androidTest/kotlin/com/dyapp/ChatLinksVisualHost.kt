package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatState

internal enum class ChatLinksVisualState(val screen: String) {
    LIVE("chat-links-live"),
    EDITED("chat-links-edited"),
    DELETED("chat-links-deleted")
}

internal class ChatLinksVisualScene(
    val state: ChatLinksVisualState,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float,
) {
    val metadata = NativeScreenVisualScene("chat-links-root", state.screen, context, dark, fontScale)
    val form = ChatForm(mutableStateOf(""))
    private val original = message(
        "peer-abcdef",
        "https://safe.example@evil.example/a https://b\u00fccher.example:8443/"
    )
    val item = when (state) {
        ChatLinksVisualState.LIVE -> original
        ChatLinksVisualState.EDITED -> original.copy(text = "https://changed.example", edited = true)
        ChatLinksVisualState.DELETED -> original.copy(deleted = true)
    }
}

/** Production chat layout and native parser, without an identity owner or peer-photo loader. */
internal class ChatLinksVisualHost(private val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<ChatLinksVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: ChatLinksVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: ChatLinksVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                CompositionLocalProvider(LocalPeerPhotos provides null) {
                    Surface(Modifier.fillMaxSize().testTag(scene.metadata.rootTag)) {
                        ChatContent(
                            "peer-abcdef",
                            "self",
                            ChatHistory(ChatState.OPEN, listOf(scene.item), false),
                            scene.form,
                            ChatActions({}, {}, {}, {}, {})
                        )
                    }
                }
            }
        }
    }
}
