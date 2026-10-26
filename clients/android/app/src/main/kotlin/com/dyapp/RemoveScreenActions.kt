package com.dyapp

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role

/** Confirmation and actions for removing an identity from the active slot list. */
@Composable
internal fun RemoveScreenActions(slots: Slots, slot: Slot, peerId: String?, act: (() -> Opened) -> Unit) {
    var confirmed by remember { mutableStateOf(false) }
    Row(
        Modifier.toggleable(confirmed, role = Role.Checkbox) { confirmed = it }
            .testTag("remove-confirm"),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Checkbox(confirmed, onCheckedChange = null)
        Text(stringResource(R.string.remove_confirm))
    }
    Button(
        onClick = { act { slots.remove(slot.id, peerId); Opened.Picker } },
        Modifier.testTag("remove-action"),
        enabled = confirmed
    ) {
        Text(stringResource(R.string.remove_action))
    }
    OutlinedButton(onClick = { act(slots::open) }, Modifier.testTag("remove-cancel")) {
        Text(stringResource(R.string.remove_cancel))
    }
}
