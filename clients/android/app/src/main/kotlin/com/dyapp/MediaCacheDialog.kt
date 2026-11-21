package com.dyapp

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ChatMediaCacheEdit

@Composable
internal fun MediaCacheDialog(state: MediaCachePresentation, confirm: (ChatMediaCacheEdit) -> Unit) {
    state.pending?.let { edit ->
        AlertDialog(
            onDismissRequest = state::cancel,
            title = { Text(stringResource(R.string.cache_confirm_title)) },
            text = { Text(stringResource(R.string.cache_confirm_body, cacheEditLabel(edit))) },
            confirmButton = {
                val action = { confirm(edit) }
                TextButton(action, Modifier.testTag("cache-confirm").applicationClick(action), enabled = !state.busy) {
                    Text(stringResource(R.string.cache_confirm))
                }
            },
            dismissButton = {
                TextButton(state::cancel, Modifier.testTag("cache-cancel"), enabled = !state.busy) {
                    Text(stringResource(R.string.cache_cancel))
                }
            }
        )
    }
}

@Composable
private fun cacheEditLabel(edit: ChatMediaCacheEdit): String = when (edit) {
    is ChatMediaCacheEdit.Retention -> stringResource(
        if (edit.enabled) R.string.cache_enable_request else R.string.cache_disable_request
    )
    is ChatMediaCacheEdit.Budget -> stringResource(R.string.cache_budget_request, edit.mib)
    ChatMediaCacheEdit.Clear -> stringResource(R.string.cache_clear)
}
