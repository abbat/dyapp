package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

@Composable
internal fun ConfirmAction(tag: String, label: Int, enabled: Boolean, action: () -> Unit) {
    TextButton(action, Modifier.testTag(tag).applicationClick(action), enabled) {
        Text(stringResource(label))
    }
}
