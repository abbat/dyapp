package com.dyapp

import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ContactNameView

/** The rename action starts with the local contact name. */
@Composable
internal fun RenameMenuItem(form: ChatForm, name: ContactNameView?, dismiss: () -> Unit) {
    // The input starts from the local name; a public name is the peer's own claim.
    DropdownMenuItem(text = { Text(stringResource(R.string.rename)) }, onClick = {
        dismiss()
        form.renameError = null
        form.renaming.value = name?.takeIf { it.verified == null }?.name.orEmpty()
    }, Modifier.testTag("chat-rename"))
}
