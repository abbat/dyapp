package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.LinkReviewSource
import uniffi.dyapp.Rgba

class LinkReviewPhotoPublicationTests {
    @Test
    fun completedReadCannotPublishPixelsAfterAnEqualFormReplacement() = runBlocking {
        val snapshot = linkReviewFixture()
        val entered = CompletableDeferred<Unit>()
        val result = CompletableDeferred<Result<Rgba>>()
        val opening = LinkReviewOpening(snapshot, { _, _ -> snapshot.local }, { _, _, _ ->
            entered.complete(Unit)
            result
        })
        val original = LinkReviewFormState(opening) { true }
        val photos = LinkReviewPhotoState(original)
        val key = LinkReviewPhotoKey(LinkReviewSource.LOCAL, 0u)
        val read = launch { photos.load(key) }
        withTimeout(5_000) { entered.await() }
        original.retire()
        val successor = LinkReviewFormState(opening) { true }
        val nextPhotos = LinkReviewPhotoState(successor)
        result.complete(Result.success(Rgba(2u, 1u, byteArrayOf(10, 20, 30, -1, 50, 60, 70, -1))))
        withTimeout(5_000) { read.join() }
        assertNull(photos.value(key))
        assertNull(nextPhotos.value(key))
    }
}
