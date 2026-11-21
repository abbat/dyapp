package com.dyapp

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

@Composable
internal fun SavedStickerManagerButton() {
    val open = LocalSavedStickerManager.current ?: return
    TextButton(open, Modifier.fillMaxWidth().testTag("saved-sticker-open").applicationClick(open)) {
        Text(stringResource(R.string.sticker_manager_title))
    }
}
