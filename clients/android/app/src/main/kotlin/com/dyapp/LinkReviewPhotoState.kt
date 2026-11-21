package com.dyapp

import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSource

internal data class LinkReviewPhotoKey(val source: LinkReviewSource, val index: UInt)

/** Bitmap presentation never owns the native read's job, key or original registry selection. */
internal class LinkReviewPhotoState(private val form: LinkReviewFormState) {
    private val values = mutableStateMapOf<LinkReviewPhotoKey, Result<ImageBitmap>>()
    fun value(key: LinkReviewPhotoKey) = values[key]

    suspend fun load(key: LinkReviewPhotoKey) {
        if (!form.present) return
        val read = form.opening.thumbnail(key.source, key.index) ?: return
        val result = read.await()
        val decoded = withContext(Dispatchers.Default) { result.mapCatching(::previewBitmap) }
        if (form.present) values[key] = decoded
    }

    suspend fun loadOffered() {
        form.loadingPhotos = true
        try {
            loadSource(LinkReviewSource.LOCAL, form.opening.snapshot.local)
            loadSource(LinkReviewSource.REMOTE, form.opening.snapshot.remote)
        } finally {
            if (form.present) form.loadingPhotos = false
        }
    }

    private suspend fun loadSource(source: LinkReviewSource, profile: LinkReviewProfile) {
        for (index in profile.photos.indices) {
            if (!form.present) return
            load(LinkReviewPhotoKey(source, index.toUInt()))
        }
    }
}
