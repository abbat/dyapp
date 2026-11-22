package com.dyapp

import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource

/** A pending or failed storage read never becomes a fresh, unprotected application session. */
@Composable
internal fun AppLockStartup(
    owner: Any,
    read: () -> StoredLock,
    initialize: (StoredLock) -> Unit,
    content: @Composable () -> Unit
) {
    val saved by rememberNativeRead(owner, read = read)
    // Scope startup to its owner without changing private content's saveable identity on recreation.
    var initialized by remember(owner) { mutableStateOf(false) }
    LaunchedEffect(owner, saved) {
        saved?.let {
            initialize(it.getOrDefault(StoredLock.PIN))
            initialized = true
        }
    }
    if (initialized) {
        content()
    } else {
        Page(actions = {}) {
            CircularProgressIndicator(Modifier.testTag("app-lock-loading"))
            Text(stringResource(R.string.view_photo_loading))
        }
    }
}
