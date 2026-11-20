package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.launch
import uniffi.dyapp.FavoriteException

private class FavoritePresentation {
    var saved by mutableStateOf<Boolean?>(null)
    var busy by mutableStateOf(true)
    var message by mutableStateOf<Int?>(null)

    fun complete(result: Result<Boolean>, changed: Boolean?) {
        result.fold(
            onSuccess = {
                saved = it
                message = when {
                    changed == null -> null
                    it -> R.string.favorites_saved
                    else -> R.string.favorites_removed
                }
            },
            onFailure = {
                message = when {
                    it is FavoriteException.Blocked -> R.string.favorites_blocked
                    changed == null -> R.string.favorites_load_failed
                    else -> R.string.favorites_change_failed
                }
            }
        )
        busy = false
    }

    fun label() = when {
        busy && saved == null -> R.string.favorites_loading
        busy -> R.string.favorites_saving
        saved == null -> R.string.favorites_retry
        saved == true -> R.string.favorites_remove
        else -> R.string.favorites_add
    }
}

/** Key with identity and profile opening; replacing it cancels the old presentation continuation. */
@Composable
internal fun FavoriteAction(owner: Any, calls: FavoriteCalls) = key(owner) {
    val state = remember { FavoritePresentation() }
    val backend = remember { calls }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.complete(backend.resolve(null), null) }
    val change = {
        // Admission is synchronous: repeated callbacks in one UI turn cannot enqueue two writes.
        if (!state.busy) {
            val target = state.saved?.not()
            state.busy = true
            state.message = null
            scope.launch { state.complete(backend.resolve(target), target) }
        }
    }
    FormCard {
        OutlinedButton(change, Modifier.fillMaxWidth().testTag("view-favorite-toggle"), enabled = !state.busy) {
            Text(stringResource(state.label()))
        }
        Caption(stringResource(R.string.favorites_hint))
        state.message?.let {
            Text(
                stringResource(it),
                Modifier.testTag("view-favorite-status").semantics { liveRegion = LiveRegionMode.Polite }
            )
        }
    }
}
