package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp

@Composable
internal fun LanguageDialog(
    options: List<LanguageOption>,
    selected: String?,
    cancel: () -> Unit,
    choose: (String?) -> Unit
) {
    AlertDialog(
        onDismissRequest = cancel,
        modifier = Modifier.testTag("language-picker"),
        title = { Text(stringResource(R.string.settings_language)) },
        text = { LanguageOptions(options, selected, choose) },
        confirmButton = {},
        dismissButton = {
            TextButton(cancel, Modifier.testTag("language-picker-cancel")) {
                Text(stringResource(R.string.language_cancel))
            }
        }
    )
}

@Composable
private fun LanguageOptions(options: List<LanguageOption>, selected: String?, choose: (String?) -> Unit) {
    val focus = remember { FocusRequester() }
    val ready = LocalWindowInfo.current.isWindowFocused
    Column(Modifier.verticalScroll(rememberScrollState()).selectableGroup()) {
        options.forEach { option -> LanguageOptionRow(option, option.tag == selected, focus) { choose(option.tag) } }
    }
    LaunchedEffect(ready) { if (ready) focus.requestFocus() }
}

@Composable
private fun LanguageOptionRow(option: LanguageOption, selected: Boolean, focus: FocusRequester, choose: () -> Unit) {
    val tag = option.tag ?: "system"
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("language-" + tag)
            .then(if (selected) Modifier.focusRequester(focus) else Modifier)
            .selectable(selected, role = Role.RadioButton, onClick = choose),
        verticalAlignment = Alignment.CenterVertically
    ) {
        RadioButton(selected, onClick = null)
        Text(option.name, Modifier.weight(1f).testTag("language-label-" + tag))
    }
}
