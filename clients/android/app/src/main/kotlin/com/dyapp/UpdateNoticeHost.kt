package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdatePlatform
import uniffi.dyapp.MobileUpdateState

internal val LocalClientUpdateState = compositionLocalOf<MobileUpdateState?> { null }

/** Lives above identity routes, and leaves composition with the unlocked MainScreen. */
@Composable
internal fun UpdateNoticeHost(visible: Boolean, content: @Composable () -> Unit) {
    val root = LocalContext.current.applicationContext.noBackupFilesDir.absolutePath
    val scope = rememberCoroutineScope()
    val actions = remember(root, scope) {
        UpdateNoticeActions(
            scope,
            read = { MobileClientUpdates(root, MobileUpdatePlatform.ANDROID).use { it.readState() } },
            dismiss = { version ->
                MobileClientUpdates(root, MobileUpdatePlatform.ANDROID).use { it.dismissNotice(version) }
            },
        )
    }
    LaunchedEffect(actions) { actions.initialize() }
    UpdateNoticeContent(actions, visible, content)
}

/** The badge and dialog consume one installation snapshot, including after Later commits. */
@Composable
internal fun UpdateNoticeContent(actions: UpdateNoticeActions, visible: Boolean, content: @Composable () -> Unit) {
    val state by actions.state.collectAsState()
    val browser = LocalUriHandler.current
    CompositionLocalProvider(LocalClientUpdateState provides state.snapshot) {
        content()
        if (visible) UpdateNoticeDialog(state, { actions.later(it) }, browser::openUri)
    }
}
