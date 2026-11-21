package com.dyapp

import android.content.ActivityNotFoundException
import android.content.Context
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts.OpenDocument
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.dyapp.stickerImportByteLimit
import java.io.IOException

/** Register throughout this opening; clearing a picker ticket must not cancel its file-read job. */
@Composable
internal fun rememberSavedStickerPicker(state: SavedStickerPresentation, calls: SavedStickerCalls): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var launched by remember { mutableStateOf<Any?>(null) }
    val picker = rememberLauncherForActivityResult(OpenDocument()) { uri ->
        launched?.let { scope.acceptPicked(state, calls, it, archiveReader(context, uri)) }
        launched = null
    }
    return {
        if (scope.isActive) {
            state.requestFile()?.let { ticket ->
                launched = ticket
                try {
                    picker.launch(arrayOf("*/*"))
                } catch (error: ActivityNotFoundException) {
                    launched = null
                    scope.acceptPicked(state, calls, ticket) { throw error }
                }
            }
        }
    }
}

private fun CoroutineScope.acceptPicked(
    state: SavedStickerPresentation,
    calls: SavedStickerCalls,
    ticket: Any,
    read: (suspend () -> ByteArray)?
) {
    if (isActive) launch(start = CoroutineStart.UNDISPATCHED) { state.importPicked(calls, ticket, read) }
}

private fun archiveReader(context: Context, uri: Uri?): (suspend () -> ByteArray)? = uri?.let { chosen ->
    {
        val input = context.contentResolver.openInputStream(chosen) ?: throw IOException("Cannot open picked archive")
        input.use { readPickedBytes(it, stickerImportByteLimit()) }
    }
}
