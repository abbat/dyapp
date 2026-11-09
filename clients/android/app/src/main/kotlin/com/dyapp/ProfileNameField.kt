package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

/** The editable public name and its disclosure. */
@Composable
internal fun ProfileNameField(name: MutableState<String>) {
    OutlinedTextField(
        name.value,
        { name.value = it },
        Modifier.fillMaxWidth().testTag("profile-name"),
        label = { Text(stringResource(R.string.profile_name)) },
        supportingText = { Text(stringResource(R.string.profile_name_hint)) },
        singleLine = true
    )
}
