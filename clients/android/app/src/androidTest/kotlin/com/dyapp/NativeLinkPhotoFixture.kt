package com.dyapp

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.addProfilePhoto
import uniffi.dyapp.makePreview

/** Actual core JPEG preparation and original-slot persistence before the existing publication path. */
internal fun prepareNativeLinkPhoto(original: Opened.Ready) {
    saveDraft(original, ProfileDraft())
    val jpeg = makePreview(byteArrayOf(10, 20, 30, -1, 50, 60, 70, -1), 2u, 1u, 24u * 1024u)
    RustBridge.withKey(original) { dir, key ->
        assertEquals(1, addProfilePhoto(dir, key, jpeg, jpeg).size)
    }
}

internal fun assertNativeReviewProjection(expected: LinkReviewProfile, actual: LinkReviewProfile) {
    assertEquals(expected.copy(photos = emptyList()), actual.copy(photos = emptyList()))
    assertEquals(expected.photos.size, actual.photos.size)
    for ((left, right) in expected.photos.zip(actual.photos)) {
        assertArrayEquals(left.thumbnail, right.thumbnail)
        assertEquals(left.full.size, right.full.size)
        for ((chunk, actualChunk) in left.full.zip(right.full)) assertArrayEquals(chunk, actualChunk)
    }
}

/** Invokes the generated photo JNI binding on the retained production review, not a bitmap fake. */
internal fun assertNativeLinkThumbnail(review: LinkEnrollmentReview) {
    assertEquals(1, review.snapshot.remote.photos.size)
    assertTrue(review.snapshot.remote.photos.single().full.isNotEmpty())
    val pixels = review.photo(LinkReviewSource.REMOTE, 0u)
    assertEquals(2u, pixels.width)
    assertEquals(1u, pixels.height)
    assertEquals(8, pixels.pixels.size)
}
