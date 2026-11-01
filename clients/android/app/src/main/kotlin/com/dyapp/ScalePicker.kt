package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileScale

@Composable
internal fun label(context: Context, key: String): String {
    val name = "profile_" + key.replace('.', '_')
    val id = context.resources.getIdentifier(name, "string", context.packageName)
    return if (id == 0) key else stringResource(id)
}

/** Picks one of the scale's choices or [none], shown with that label. */
@Composable
internal fun ScalePicker(scale: ProfileScale, selected: Int, none: Int, onSelect: (Int) -> Unit) {
    val context = LocalContext.current
    var expanded by remember { mutableStateOf(false) }
    // The prototype names this one control profile-goal; the key itself is a stored name.
    val tag = "profile-${scale.key.removePrefix("relationship_")}"
    Column(Modifier.fillMaxWidth(), Arrangement.spacedBy(4.dp)) {
        Caption(label(context, "field.${scale.key}"))
        Box {
            OutlinedButton(onClick = { expanded = true }, Modifier.fillMaxWidth().testTag(tag)) {
                val chosen = scale.choices.firstOrNull { it.code.toInt() == selected }
                val title = if (chosen == null) {
                    stringResource(none)
                } else {
                    label(context, "value.${scale.key}.${chosen.key}")
                }
                Text(title)
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                DropdownMenuItem(text = { Text(stringResource(none)) },
                    onClick = { onSelect(0); expanded = false })
                for (choice in scale.choices) {
                    DropdownMenuItem(text = { Text(label(context, "value.${scale.key}.${choice.key}")) },
                        onClick = { onSelect(choice.code.toInt()); expanded = false })
                }
            }
        }
    }
}
