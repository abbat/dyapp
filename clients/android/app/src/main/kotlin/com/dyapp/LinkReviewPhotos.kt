package com.dyapp

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.dyapp.LinkReviewPhoto
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSource

private const val HASH_RADIX = 16
private val LinkReviewPhotoKey.tag get() = "link-review-photo-$source-$index"

@Composable
internal fun LinkReviewPhotoList(
    form: LinkReviewFormState,
    photos: LinkReviewPhotoState,
    source: LinkReviewSource,
    profile: LinkReviewProfile
) {
    for ((index, photo) in profile.photos.withIndex()) {
        LinkReviewPhotoRow(form, photos, LinkReviewPhotoKey(source, index.toUInt()), photo)
    }
}

@Composable
private fun LinkReviewPhotoRow(
    form: LinkReviewFormState,
    photos: LinkReviewPhotoState,
    key: LinkReviewPhotoKey,
    photo: LinkReviewPhoto
) {
    val scope = rememberCoroutineScope()
    val result = photos.value(key)
    val bitmap = result?.getOrNull()
    Text(stringResource(R.string.link_review_photo_number, key.index.toInt() + 1))
    if (bitmap != null) {
        Image(
            bitmap,
            stringResource(R.string.link_review_photo_number, key.index.toInt() + 1),
            Modifier.size(96.dp).testTag(key.tag),
            contentScale = ContentScale.Fit
        )
    } else {
        Caption(
            stringResource(
                if (result == null) R.string.link_review_photo_not_loaded else R.string.link_review_photo_unavailable
            )
        )
        val retry: () -> Unit = { if (form.available) scope.launch { photos.load(key) } }
        Button(
            retry,
            Modifier.testTag("${key.tag}-retry").applicationClick(retry),
            form.available
        ) {
            Text(stringResource(R.string.link_review_photo_retry))
        }
    }
    LinkReviewPhotoReferences(key, photo)
}

@Composable
private fun LinkReviewPhotoReferences(key: LinkReviewPhotoKey, photo: LinkReviewPhoto) {
    var expanded by remember(key) { mutableStateOf(false) }
    val toggle = { expanded = !expanded }
    val tag = key.tag
    TextButton(toggle, Modifier.testTag("$tag-references").applicationClick(toggle)) {
        Text(stringResource(R.string.link_review_photo_references))
    }
    if (expanded) {
        LinkReviewHash(stringResource(R.string.link_review_photo_thumbnail_hash), photo.thumbnail, "$tag-thumbnail")
        for ((index, hash) in photo.full.withIndex()) {
            LinkReviewHash(
                stringResource(R.string.link_review_photo_chunk_hash, index + 1),
                hash,
                "$tag-original-$index"
            )
        }
    }
}

@Composable
private fun LinkReviewHash(title: String, bytes: ByteArray, tag: String) {
    val hex = remember(bytes) { bytes.joinToString("") { it.toUByte().toString(HASH_RADIX).padStart(2, '0') } }
    Caption(title)
    Text(hex, Modifier.testTag(tag))
    CopyButton(hex, "$tag-copy")
}
