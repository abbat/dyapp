package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import uniffi.dyapp.MobileClientUpdates
import uniffi.dyapp.MobileUpdatePlatform

/** Lives above identity routes, and leaves composition with the unlocked MainScreen. */
@Composable
internal fun UpdateNoticeHost(visible: Boolean) {
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
    val state by actions.state.collectAsState()
    val browser = LocalUriHandler.current
    if (visible) UpdateNoticeDialog(state, { actions.later(it) }, browser::openUri)
}
