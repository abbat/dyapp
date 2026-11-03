package com.dyapp

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.dyapp.FoundProfile
import uniffi.dyapp.ProfileSearcher
import uniffi.dyapp.checkPreview
import uniffi.dyapp.peerPhotoAuto

private const val FULL_SIDE = 2048

/**
 * Other people's photos in the searcher's memory cache; see docs/design/profile-photos.md. A page
 * fetches its photos in one call, then each photo decodes from the cache, so the decoded bitmaps
 * live only as long as what shows them.
 */
internal class PeerPhotos(private val searcher: ProfileSearcher?, private val opened: Opened.Ready) {
    /** Counts finished fetches, so a photo shown before one reads the cache again after it. */
    var fetched by mutableIntStateOf(0)
        private set

    /** Fetches what the cache lacks; a failure leaves the letters, and a later view tries again. */
    suspend fun fetch(hashes: List<ByteArray>, full: Boolean) {
        val wanted = hashes.filter { it.isNotEmpty() }
        if (searcher == null || wanted.isEmpty()) return
        withContext(Dispatchers.IO) { runCatching { searcher.fetchPeerPhotos(wanted, full) } }
        fetched += 1
    }

    /** Whether full images load without a tap: Settings Photos and the metered report. */
    suspend fun auto(): Boolean = withContext(Dispatchers.IO) {
        runCatching { RustBridge.withKey(opened) { dir, key -> peerPhotoAuto(dir, key) } }.getOrDefault(false)
    }

    /** The cached photo decoded, or null when it is missing or fails its checks. Call off the UI thread. */
    fun decoded(hash: ByteArray, full: Boolean): ImageBitmap? = runCatching {
        val bytes = checkNotNull(searcher).peerPhoto(hash)
        if (full) fullImage(bytes) else thumbnail(bytes)
    }.getOrNull()
}

/** The identity's peer photos; null in screens shown without a shell, which then keep the letters. */
internal val LocalPeerPhotos = staticCompositionLocalOf<PeerPhotos?> { null }

// The core decodes thumbnails, held to 320 x 320, so a crafted JPEG never reaches the platform decoder.
private fun thumbnail(bytes: ByteArray): ImageBitmap {
    val rgba = checkPreview(bytes)
    val bitmap = Bitmap.createBitmap(rgba.width.toInt(), rgba.height.toInt(), Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba.pixels))
    return bitmap.asImageBitmap()
}

// The cache admits a full image within 64 megapixels; it decodes at most 2048 px on the long side.
private fun fullImage(bytes: ByteArray): ImageBitmap =
    ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
        fitted(info.size.width, info.size.height, FULL_SIDE)?.let { (w, h) -> decoder.setTargetSize(w, h) }
    }.asImageBitmap()

/** [hash] decoded from the cache, read again after each fetch; null while missing. */
@Composable
internal fun rememberPeerPhoto(hash: ByteArray?, full: Boolean = false): ImageBitmap? {
    val photos = LocalPeerPhotos.current
    val photo by produceState<ImageBitmap?>(null, photos, hash?.toList(), photos?.fetched) {
        val wanted = hash?.takeIf { it.isNotEmpty() }
        value = withContext(Dispatchers.IO) { wanted?.let { photos?.decoded(it, full) } }
    }
    return photo
}

/** Fetches the first thumbnail of each of [found] in one call, for a list of cards. */
@Composable
internal fun FetchCardPhotos(found: List<FoundProfile>) {
    val photos = LocalPeerPhotos.current ?: return
    LaunchedEffect(photos, found) { photos.fetch(found.mapNotNull { it.photos.firstOrNull() }, false) }
}

/** The choice of photo and the state of a tap on Load full photo. */
private class GalleryState {
    var chosen by mutableIntStateOf(0)
    var auto by mutableStateOf(false)
    var loading by mutableStateOf(false)
    var failed by mutableStateOf(false)
}

/**
 * All of [found]'s photos: the chosen one large and the thumbnails to choose from. A full image
 * loads by itself only when Settings Photos allows it now; otherwise Load full photo asks for it.
 */
@Composable
internal fun PeerGallery(found: FoundProfile) {
    val photos = LocalPeerPhotos.current
    val count = found.photos.size
    if (photos == null || count == 0) return
    val state = remember(found.peerId) { GalleryState() }
    val full = found.fullPhotos.getOrNull(state.chosen)?.takeIf { it.isNotEmpty() }
    GalleryFetch(photos, found, state, full)
    val large = rememberPeerPhoto(full, full = true)
    MainPhoto(found, state, large ?: rememberPeerPhoto(found.photos[state.chosen]))
    if (count > 1) GalleryThumbnails(found, state)
    if (full != null && large == null && !state.auto) LoadFullPhoto(photos, full, state)
}

// Every thumbnail at once, then the chosen full image when it may load without a tap.
@Composable
private fun GalleryFetch(photos: PeerPhotos, found: FoundProfile, state: GalleryState, full: ByteArray?) {
    LaunchedEffect(found) {
        photos.fetch(found.photos, false)
        state.auto = photos.auto()
    }
    LaunchedEffect(full?.toList(), state.auto) {
        if (state.auto && full != null) photos.fetch(listOf(full), true)
    }
}

@Composable
private fun MainPhoto(found: FoundProfile, state: GalleryState, shown: ImageBitmap?) {
    val label = stringResource(R.string.profile_photo_label, state.chosen + 1, found.photos.size)
    val box = Modifier.fillMaxWidth().aspectRatio(1f).background(MaterialTheme.colorScheme.surfaceVariant)
    Box(box.testTag("view-photo"), contentAlignment = Alignment.Center) {
        if (shown != null) {
            Image(shown, label, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
        } else {
            Text(found.peerId.take(2), style = MaterialTheme.typography.headlineMedium)
        }
    }
}

@Composable
private fun GalleryThumbnails(found: FoundProfile, state: GalleryState) {
    val count = found.photos.size
    Row(Modifier.horizontalScroll(rememberScrollState()), Arrangement.spacedBy(8.dp)) {
        found.photos.forEachIndexed { index, hash ->
            val label = stringResource(R.string.profile_photo_label, index + 1, count)
            Surface(
                onClick = { state.chosen = index; state.failed = false },
                modifier = Modifier.testTag("view-photo-$index").semantics {
                    contentDescription = label
                    selected = index == state.chosen
                },
                color = MaterialTheme.colorScheme.surfaceVariant,
            ) {
                ThumbnailTile(rememberPeerPhoto(hash), found.peerId)
            }
        }
    }
}

// The button carries the description; the tile is drawn only.
@Composable
private fun ThumbnailTile(photo: ImageBitmap?, peerId: String) =
    Box(Modifier.size(56.dp), contentAlignment = Alignment.Center) {
        if (photo != null) {
            Image(photo, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            Text(peerId.take(2))
        }
    }

@Composable
private fun LoadFullPhoto(photos: PeerPhotos, full: ByteArray, state: GalleryState) {
    val scope = rememberCoroutineScope()
    OutlinedButton(
        {
            state.loading = true
            state.failed = false
            scope.launch {
                photos.fetch(listOf(full), true)
                state.failed = withContext(Dispatchers.IO) { photos.decoded(full, true) == null }
                state.loading = false
            }
        },
        Modifier.fillMaxWidth().testTag("view-photo-load"),
        enabled = !state.loading,
    ) { Text(stringResource(if (state.loading) R.string.view_photo_loading else R.string.view_photo_load)) }
    if (state.failed) {
        Caption(stringResource(R.string.view_photo_failed),
            Modifier.testTag("view-photo-status").semantics { liveRegion = LiveRegionMode.Polite })
    }
}
