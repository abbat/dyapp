package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The privacy card's read receipt switch for the active identity, off by default. It stays disabled
 * until the stored value is read; a failed read or save says so and keeps the last value.
 */
@Composable
internal fun ReadReceiptsRow(active: Opened.Ready) {
    var on by remember(active.slot.id) { mutableStateOf<Boolean?>(null) }
    var failed by remember(active.slot.id) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(active.slot.id) {
        val loaded = withContext(Dispatchers.IO) {
            runCatching { RustBridge.withChats(active) { it.readReceipts() } }.getOrNull()
        }
        failed = loaded == null
        on = loaded
    }
    ReadReceiptsSwitch(on, failed) { wanted ->
        scope.launch {
            val saved = withContext(Dispatchers.IO) {
                runCatching { RustBridge.withChats(active) { it.setReadReceipts(wanted) } }.isSuccess
            }
            failed = !saved
            if (saved) on = wanted
        }
    }
}

@Composable
private fun ReadReceiptsSwitch(on: Boolean?, failed: Boolean, save: (Boolean) -> Unit) = Column {
    Row(
        Modifier.fillMaxWidth().testTag("send-read-receipts").toggleable(
            on == true, enabled = on != null, role = Role.Switch, onValueChange = save
        ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(R.string.receipts_toggle), Modifier.weight(1f))
        Switch(on == true, onCheckedChange = null)
    }
    Caption(stringResource(R.string.receipts_hint))
    if (failed) {
        Caption(
            stringResource(R.string.receipts_failed),
            Modifier.testTag("receipts-error").semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
}
