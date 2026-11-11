package com.dyapp

import android.content.Context
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
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
import uniffi.dyapp.ProfilePhotoSnapshot
import uniffi.dyapp.addProfilePhoto
import uniffi.dyapp.profilePhotoSnapshot

/** The schema allows 16; six fit the node quota with room for chats. */
internal const val MAX_PROFILE_PHOTOS = 6

private fun added(context: Context, opened: Opened.Ready, uri: Uri): ProfilePhotoSnapshot {
    val photo = preparePhoto(readPicked(context, uri))
    return withPhotoDraft(opened) { dir, key ->
        addProfilePhoto(dir, key, photo.full, photo.thumbnail)
        profilePhotoSnapshot(dir, key)
    }
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
    LaunchedEffect(state) { state.run(scope, done = null) { withPhotoDraft(opened, ::profilePhotoSnapshot) } }
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
    Modifier.testTag("profile-photo-$index"),
    horizontalAlignment = Alignment.CenterHorizontally
) {
    val count = state.photos.size
    val label = stringResource(R.string.profile_photo_label, index + 1, count)
    // An unreadable thumbnail leaves the tinted square, as an unloaded avatar does.
    val tile = Modifier.size(96.dp).background(MaterialTheme.colorScheme.surfaceVariant)
    Box(tile.semantics { contentDescription = label }) {
        photo?.let { Image(it, null, Modifier.size(96.dp), contentScale = ContentScale.Crop) }
    }
    if (index == 0) Caption(stringResource(R.string.profile_photo_main))
    val expected = checkNotNull(state.rows).snapshot.revision
    val move = { to: Int -> state.move(scope, expected, index, to) }
    val number = index + 1
    Row {
        TileButton("\u2190", stringResource(R.string.profile_photo_left, number), !state.busy && index > 0) {
            move(index - 1)
        }
        TileButton("\u2192", stringResource(R.string.profile_photo_right, number), !state.busy && number < count) {
            move(number)
        }
        TileButton("\u00D7", stringResource(R.string.profile_photo_remove, number), !state.busy) {
            state.removing = PhotoRemoval(index.toUInt(), expected)
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
private fun RemovePhotoDialog(state: PhotosState, scope: CoroutineScope, removal: PhotoRemoval) = AlertDialog(
    onDismissRequest = { state.removing = null },
    title = { Text(stringResource(R.string.profile_photo_remove_title)) },
    text = { Text(stringResource(R.string.profile_photo_remove_text)) },
    confirmButton = {
        TextButton(
            { state.remove(scope, removal) },
            Modifier.testTag("profile-photo-remove-confirm"),
            colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
        ) { Text(stringResource(R.string.profile_photo_remove_action)) }
    },
    dismissButton = { TextButton({ state.removing = null }) { Text(stringResource(R.string.remove_cancel)) } },
)
