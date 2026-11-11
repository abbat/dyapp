package com.dyapp

import android.graphics.BitmapFactory
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.IdentityKey
import uniffi.dyapp.ProfilePhotoEdit
import uniffi.dyapp.ProfilePhotoEditException
import uniffi.dyapp.ProfilePhotoSnapshot
import uniffi.dyapp.editProfilePhotos
import uniffi.dyapp.profilePhoto
import uniffi.dyapp.profilePhotoSnapshot
import uniffi.dyapp.profilePhotos

// Counts photo changes in the editor, so the own avatar reads the draft again after one.
private var photoEdits by mutableIntStateOf(0)

internal class PhotoRows(
    val snapshot: ProfilePhotoSnapshot,
    val images: List<ImageBitmap?>,
    val conflict: Boolean
)

internal class PhotoRemoval(val index: UInt, val revision: ByteArray)

/** Images and opaque revision publish together; the UI never computes photo identity. */
internal class PhotosState(val opened: Opened.Ready) {
    var rows by mutableStateOf<PhotoRows?>(null)
    val photos get() = rows?.images.orEmpty()
    var busy by mutableStateOf(true)
    var status by mutableStateOf<Int?>(null)
    var removing by mutableStateOf<PhotoRemoval?>(null)

    fun move(scope: CoroutineScope, expected: ByteArray, from: Int, to: Int) {
        run(scope) {
            withPhotoDraft(opened) { dir, key ->
                editProfilePhotos(dir, key, expected, ProfilePhotoEdit.Move(from.toUInt(), to.toUInt()))
            }
        }
    }

    fun remove(scope: CoroutineScope, removal: PhotoRemoval) {
        removing = null
        run(scope) {
            withPhotoDraft(opened) { dir, key ->
                editProfilePhotos(dir, key, removal.revision, ProfilePhotoEdit.Remove(removal.index))
            }
        }
    }

    /** Runs [change] off the UI thread; [done] is the status it leaves, or null to keep the line. */
    fun run(scope: CoroutineScope, done: Int? = R.string.profile_photo_changed, change: () -> ProfilePhotoSnapshot) {
        busy = true
        status = R.string.profile_photo_working.takeIf { done != null }
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { photoRows(opened, change) } }
                .fold({
                    rows = it
                    status = if (it.conflict) R.string.profile_photo_conflict else done
                    photoEdits += 1
                }, { status = photoError(it) })
            busy = false
        }
    }
}

// Keep the frontend monitor until publication uses shared core transactions.
internal fun <T> withPhotoDraft(opened: Opened.Ready, action: (String, IdentityKey) -> T): T =
    synchronized(RustBridge.draftWrites) { RustBridge.withKey(opened, action) }

private fun thumbnails(opened: Opened.Ready, hashes: List<ByteArray>): List<ImageBitmap?> =
    RustBridge.withKey(opened) { dir, key ->
        hashes.map { hash ->
            runCatching { profilePhoto(dir, key, hash) }.getOrNull()
                ?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
        }
    }

private fun photoRows(opened: Opened.Ready, change: () -> ProfilePhotoSnapshot): PhotoRows {
    var conflict = false
    val snapshot = try {
        change()
    } catch (_: ProfilePhotoEditException.Changed) {
        conflict = true
        withPhotoDraft(opened, ::profilePhotoSnapshot)
    }
    return PhotoRows(snapshot, thumbnails(opened, snapshot.thumbnails), conflict)
}

/** The draft's main photo for the own avatar; null without photos, so the letters show. */
@Composable
internal fun rememberOwnPhoto(opened: Opened.Ready): ImageBitmap? {
    val photo by produceState<ImageBitmap?>(null, opened.slot.id, photoEdits) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val hashes = withPhotoDraft(opened, ::profilePhotos)
                thumbnails(opened, hashes.take(1)).firstOrNull()
            }.getOrNull()
        }
    }
    return photo
}

private fun photoError(error: Throwable): Int {
    // Structured UniFFI messages format diagnostics; read the declared payload instead.
    val reason = (error as? ProfilePhotoEditException.Failed)?.reason ?: error.message
    return when {
        error is PhotoProblem -> error.text
        reason == "photoRefused" -> R.string.profile_photo_refused
        reason == "corrupted" -> R.string.profile_photo_unsupported
        else -> R.string.profile_photo_failed
    }
}
