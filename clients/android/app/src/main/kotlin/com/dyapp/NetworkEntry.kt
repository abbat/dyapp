package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.NetworkFile

/** Whether this ready identity still needs its first network connection. */
@Composable
internal fun rememberNetworkEntryNeeded(opened: Opened.Ready?): Boolean? {
    var needed by remember(opened?.slot?.id) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(opened?.slot?.id) {
        if (opened == null) return@LaunchedEffect
        val loaded = withContext(Dispatchers.IO) {
            runCatching { RustBridge.loadNetwork(opened, NetworkFile.ACTIVE) }
        }
        needed = loaded.fold(onSuccess = { it == null }, onFailure = { true })
    }
    return needed
}
