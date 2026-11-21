package com.dyapp

import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.testTag
import uniffi.dyapp.ChatHistory

/** Receiver history rendering only; the actual registered app remains the sender behind this view. */
internal class NativeForwardReceiverView(private val activity: ComponentActivity) {
    private val view = ComposeView(activity).apply { id = View.generateViewId() }

    fun show(peer: String, me: String, history: ChatHistory) {
        activity.addContentView(
            view,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        )
        view.setContent {
            MaterialTheme {
                Surface(Modifier.fillMaxSize().testTag("native-forward-receiver")) {
                    ChatContent(peer, me, history, rememberChatForm(), ChatActions({}, {}, {}, {}, {}))
                }
            }
        }
    }

    fun close() {
        view.disposeComposition()
        (view.parent as? ViewGroup)?.removeView(view)
    }
}
