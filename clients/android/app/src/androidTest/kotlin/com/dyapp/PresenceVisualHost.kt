package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.CompletableDeferred
import uniffi.dyapp.ChatHistory
import uniffi.dyapp.ChatListPreview
import uniffi.dyapp.ChatPreviewBody
import uniffi.dyapp.ChatRow
import uniffi.dyapp.ChatState

internal const val PRESENCE_VISUAL_PEER = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

internal enum class PresenceVisualState(val screen: String) {
    LIST_ACTIVE("activity-list-active"),
    LIST_UNKNOWN("activity-list-unknown"),
    CHAT_ACTIVE("activity-chat-active"),
    CHAT_UNKNOWN("activity-chat-unknown"),
    MATCHES("activity-matches"),
    NOBODY("activity-nobody"),
    LOADING("activity-loading"),
    ERROR("activity-error")
}

internal class PresenceVisualScene(
    val state: PresenceVisualState,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float,
) {
    val metadata = NativeScreenVisualScene("activity-root", state.screen, context, dark, fontScale)
    val route = mutableStateOf(SettingsPane.PRIVACY)
    val form = ChatForm(mutableStateOf(""))
    val entered = CompletableDeferred<Unit>()
    val calls = PrivacyPreferenceCalls(read = {
        entered.complete(Unit)
        if (state == PresenceVisualState.LOADING) CompletableDeferred<Boolean>().await()
        check(state != PresenceVisualState.ERROR) { "Controlled audience read failure" }
        state == PresenceVisualState.MATCHES
    }, write = { error("Visual capture cannot change the audience") })
    val presence = ChatPresence().apply {
        read {
            check(state == PresenceVisualState.LIST_ACTIVE || state == PresenceVisualState.CHAT_ACTIVE) {
                "Controlled unknown presence"
            }
            listOf(PRESENCE_VISUAL_PEER)
        }
    }
}

/** Real production components; no identity/network owner exists, and profile-photo reads stay absent. */
internal class PresenceVisualHost(private val compose: ComposeContentTestRule) {
    private val current = mutableStateOf<PresenceVisualScene?>(null)

    init {
        compose.setContent { current.value?.let { scene -> key(scene) { Scene(scene) } } }
    }

    fun show(scene: PresenceVisualScene) {
        compose.runOnIdle { current.value = scene }
        compose.waitForIdle()
    }

    fun hide() {
        compose.runOnIdle { current.value = null }
        compose.waitForIdle()
    }

    @Composable
    private fun Scene(scene: PresenceVisualScene) {
        LocalizedTestContext(scene.context, scene.fontScale) {
            DYAppTheme(scene.dark) {
                CompositionLocalProvider(
                    LocalActiveChatPeers provides scene.presence.peers,
                    LocalPeerPhotos provides null,
                ) {
                    Surface(Modifier.fillMaxSize().testTag("activity-root")) { Content(scene) }
                }
            }
        }
    }

    @Composable
    private fun Content(scene: PresenceVisualScene) {
        when (scene.state) {
            PresenceVisualState.LIST_ACTIVE, PresenceVisualState.LIST_UNKNOWN -> ListRow()
            PresenceVisualState.CHAT_ACTIVE, PresenceVisualState.CHAT_UNKNOWN -> ChatContent(
                PRESENCE_VISUAL_PEER,
                "self",
                ChatHistory(ChatState.OPEN, emptyList(), false),
                scene.form,
                ChatActions({}, {}, {}, {}, {})
            )
            else -> SettingsFrame(scene.route) { PresencePicker(scene, scene.calls) }
        }
    }

    @Composable
    private fun ListRow() {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            ChatListRow(
                ChatRow(
                    PRESENCE_VISUAL_PEER,
                    ChatState.OPEN,
                    null,
                    ChatListPreview(ChatPreviewBody.Empty, false, null),
                    0uL,
                    false
                ),
                null,
                Modifier,
            )
        }
    }
}
