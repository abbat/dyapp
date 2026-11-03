package com.dyapp

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts.PickVisualMedia
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.addProfilePhoto
import uniffi.dyapp.moveProfilePhoto
import uniffi.dyapp.profilePhoto
import uniffi.dyapp.profilePhotos
import uniffi.dyapp.removeProfilePhoto

/** The schema allows 16; six fit the node quota with room for chats. */
internal const val MAX_PROFILE_PHOTOS = 6

/** The draft's photos as shown: one decoded thumbnail per photo, null when it cannot be read. */
private class PhotosState(val opened: Opened.Ready) {
    var photos by mutableStateOf<List<ImageBitmap?>>(emptyList())
    var busy by mutableStateOf(true)
    var status by mutableStateOf<Int?>(null)
    var removing by mutableStateOf<Int?>(null)

    /** Runs [change] off the UI thread; [done] is the status it leaves, or null to keep the line. */
    fun run(scope: CoroutineScope, done: Int? = R.string.profile_photo_changed, change: () -> List<ByteArray>) {
        busy = true
        status = R.string.profile_photo_working.takeIf { done != null }
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { thumbnails(opened, change()) } }
                .fold({ photos = it; status = done }, { status = photoError(it) })
            busy = false
        }
    }
}

// Every draft write shares the lock, so a proof step or publish cannot drop this change.
private fun <T> locked(opened: Opened.Ready, action: (String, IdentityKey) -> T): T =
    synchronized(RustBridge.draftWrites) { RustBridge.withKey(opened, action) }

private fun thumbnails(opened: Opened.Ready, hashes: List<ByteArray>): List<ImageBitmap?> =
    RustBridge.withKey(opened) { dir, key ->
        hashes.map { hash ->
            runCatching { profilePhoto(dir, key, hash) }.getOrNull()
                ?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
        }
    }

private fun added(context: Context, opened: Opened.Ready, uri: Uri): List<ByteArray> {
    val photo = preparePhoto(readPicked(context, uri))
    return locked(opened) { dir, key -> addProfilePhoto(dir, key, photo.full, photo.thumbnail) }
}

private fun photoError(error: Throwable) = when {
    error is PhotoProblem -> error.text
    error.message == "photoRefused" -> R.string.profile_photo_refused
    error.message == "corrupted" -> R.string.profile_photo_unsupported
    else -> R.string.profile_photo_failed
}

/** The draft's photos, saved as soon as they change; the first one is the main photo. */
@Composable
internal fun PhotosCard(opened: Opened.Ready) = FormCard(Modifier.testTag("profile-photos")) {
    val state = remember(opened.slot.id) { PhotosState(opened) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val pick = rememberLauncherForActivityResult(PickVisualMedia()) { uri ->
        uri?.let { state.run(scope) { added(context, opened, it) } }
    }
    LaunchedEffect(state) { state.run(scope, done = null) { locked(opened, ::profilePhotos) } }
    CardTitle(R.string.profile_photos)
    val full = state.photos.size >= MAX_PROFILE_PHOTOS
    Caption(stringResource(if (full) R.string.profile_photo_limit else R.string.profile_photo_hint))
    PhotoTiles(state, scope)
    OutlinedButton(
        { pick.launch(PickVisualMediaRequest(PickVisualMedia.ImageOnly)) },
        Modifier.testTag("profile-photo-add"),
        enabled = !state.busy && !full,
    ) { Text(stringResource(R.string.profile_photo_add)) }
    state.status?.let {
        Text(
            stringResource(it),
            Modifier.testTag("profile-photo-status").semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
    state.removing?.let { RemovePhotoDialog(state, scope, it) }
}

@Composable
private fun PhotoTiles(state: PhotosState, scope: CoroutineScope) {
    if (state.photos.isEmpty()) return
    Row(Modifier.horizontalScroll(rememberScrollState()), Arrangement.spacedBy(12.dp)) {
        state.photos.forEachIndexed { index, photo -> PhotoTile(state, scope, index, photo) }
    }
}

@Composable
private fun PhotoTile(state: PhotosState, scope: CoroutineScope, index: Int, photo: ImageBitmap?) = Column(
    Modifier.testTag("profile-photo-$index"), horizontalAlignment = Alignment.CenterHorizontally
) {
    val count = state.photos.size
    val label = stringResource(R.string.profile_photo_label, index + 1, count)
    // An unreadable thumbnail leaves the tinted square, as an unloaded avatar does.
    val tile = Modifier.size(96.dp).background(MaterialTheme.colorScheme.surfaceVariant)
    Box(tile.semantics { contentDescription = label }) {
        photo?.let { Image(it, null, Modifier.size(96.dp), contentScale = ContentScale.Crop) }
    }
    if (index == 0) Caption(stringResource(R.string.profile_photo_main))
    val move = { to: Int ->
        state.run(scope) { locked(state.opened) { d, k -> moveProfilePhoto(d, k, index.toUInt(), to.toUInt()) } }
    }
    val number = index + 1
    Row {
        TileButton("\u2190", stringResource(R.string.profile_photo_left, number), !state.busy && index > 0) {
            move(index - 1)
        }
        TileButton("\u2192", stringResource(R.string.profile_photo_right, number), !state.busy && number < count) {
            move(number)
        }
        TileButton("\u00D7", stringResource(R.string.profile_photo_remove, number), !state.busy) {
            state.removing = index
        }
    }
}

// The glyph is drawn only; the description names the action and the photo for TalkBack.
@Composable
private fun TileButton(glyph: String, description: String, enabled: Boolean, onClick: () -> Unit) =
    IconButton(onClick, enabled = enabled) {
        Text(glyph, Modifier.clearAndSetSemantics { contentDescription = description })
    }

@Composable
private fun RemovePhotoDialog(state: PhotosState, scope: CoroutineScope, index: Int) = AlertDialog(
    onDismissRequest = { state.removing = null },
    title = { Text(stringResource(R.string.profile_photo_remove_title)) },
    text = { Text(stringResource(R.string.profile_photo_remove_text)) },
    confirmButton = {
        TextButton(
            {
                state.removing = null
                state.run(scope) { locked(state.opened) { d, k -> removeProfilePhoto(d, k, index.toUInt()) } }
            },
            Modifier.testTag("profile-photo-remove-confirm"),
            colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
        ) { Text(stringResource(R.string.profile_photo_remove_action)) }
    },
    dismissButton = { TextButton({ state.removing = null }) { Text(stringResource(R.string.remove_cancel)) } },
)
