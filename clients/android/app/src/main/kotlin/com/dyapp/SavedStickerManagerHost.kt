package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

internal val LocalSavedStickerManager = staticCompositionLocalOf<(() -> Unit)?> { null }
internal val LocalSavedStickerRevision = staticCompositionLocalOf { 0 }

/** The manager is shared by Settings/chat routes, with a fresh owner for every opening. */
@Composable
internal fun SavedStickerManagerHost(
    owner: Any,
    calls: SavedStickerCalls,
    content: @Composable () -> Unit
) {
    key(owner) {
        var opening by remember { mutableStateOf<Any?>(null) }
        var revision by remember { mutableIntStateOf(0) }
        val scope = rememberCoroutineScope()
        val open: () -> Unit = { if (scope.isActive && opening == null) opening = Any() }
        CompositionLocalProvider(
            LocalSavedStickerManager provides open,
            LocalSavedStickerRevision provides revision,
            content = content
        )
        opening?.let { token ->
            SavedStickerDialog(token, calls) {
                if (scope.isActive && opening === token) {
                    opening = null
                    revision += 1
                }
            }
        }
    }
}

@Composable
private fun SavedStickerDialog(opening: Any, calls: SavedStickerCalls, close: () -> Unit) = key(opening) {
    val state = remember { SavedStickerPresentation() }
    val scope = rememberCoroutineScope()
    DisposableEffect(state) { onDispose { state.retire() } }
    LaunchedEffect(state) { state.refresh(calls) }
    val pick = rememberSavedStickerPicker(state, calls)
    val actions = SavedStickerActions(
        pick = pick,
        navigate = { after, shown ->
            if (scope.isActive) scope.launch(start = CoroutineStart.UNDISPATCHED) { state.refresh(calls, after, shown) }
        },
        confirm = { shown ->
            if (scope.isActive) scope.launch(start = CoroutineStart.UNDISPATCHED) { state.confirmRemove(calls, shown) }
        },
        close = {
            state.retire()
            close()
        }
    )
    SavedStickerManagerView(state, actions)
}
