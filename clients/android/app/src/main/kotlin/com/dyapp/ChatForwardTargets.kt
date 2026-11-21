package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import uniffi.dyapp.ContactNameView
import uniffi.dyapp.LocalCollectionEntry

@Composable
internal fun ChatForwardTargets(state: ChatForwardPresentation, actions: ChatForwardActions) {
    val targets = state.targets ?: return
    val names = rememberContactNames(targets)
    state.chosen?.let { selected ->
        Text(contactLabel(selected, names[selected]), Modifier.testTag("forward-selected-name"))
        Text(stringResource(R.string.search_peer, selected), Modifier.testTag("forward-selected-peer"))
    }
    val entries = targets.map { LocalCollectionEntry(it, names[it]?.name.orEmpty()) }
    LocalCollectionView(actions.shown, entries, actions::refresh) { entry, _ ->
        ForwardTargetRow(entry, names[entry.target], state.chosen == entry.target) { actions.choose(entry.target) }
    }
}

@Composable
private fun ForwardTargetRow(
    entry: LocalCollectionEntry,
    name: ContactNameView?,
    selected: Boolean,
    select: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().testTag("forward-target-" + entry.target)
            .applicationClick(select)
            .selectable(selected, role = Role.RadioButton, onClick = select),
        verticalAlignment = Alignment.CenterVertically
    ) {
        RadioButton(selected, onClick = null)
        Column(Modifier.weight(1f)) {
            Text(contactLabel(entry.target, name))
            Text(stringResource(R.string.search_peer, entry.target))
        }
    }
}
