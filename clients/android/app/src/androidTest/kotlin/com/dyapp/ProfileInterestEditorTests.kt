package com.dyapp

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.profileInterestLimits
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

private const val INTEREST_SAVE_DEADLINE_MS = 60_000L

class ProfileInterestEditorTests {
    @Test
    fun editsUseCoreCanonicalizationAndBlankExplicitlyClears() = runBlocking {
        val editor = ProfileEditor(ProfileDraft(interests = listOf("music")), "en")
        editor.interests.value = " Tea, Coffee \n\n MUSIC "
        assertEquals(
            R.string.profile_saved,
            editor.save { assertEquals(listOf("tea, coffee", "music"), it.interests) }
        )
        assertFalse(editor.unsaved)
        editor.interests.value = " \n "
        assertEquals(R.string.profile_saved, editor.save { assertEquals(emptyList<String>(), it.interests) })
        assertFalse(editor.unsaved)
        assertEquals(R.string.profile_saved, editor.save { assertNull(it.interests) })
    }

    @Test
    fun invalidInputRefusesWholeSaveAndUntouchedTagsRemainUnmentioned() = runBlocking {
        val editor = ProfileEditor(ProfileDraft(interests = listOf("music")), "en")
        editor.name.value = "Changed"
        assertEquals(R.string.profile_saved, editor.save { assertNull(it.interests) })
        editor.interests.value = "x".repeat(profileInterestLimits().maxTagChars.toInt() + 1)
        var writes = 0
        assertEquals(R.string.profile_interests_invalid, editor.save { writes++ })
        assertEquals(0, writes)
        assertTrue(editor.unsaved)
    }

    @Test
    fun lateSuccessRetainsNewInputAndAnUndoStillReplacesTheCommittedTags() = runBlocking {
        val editor = ProfileEditor(ProfileDraft(interests = listOf("original")), "en")
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        editor.interests.value = "first"
        val saving = async(start = CoroutineStart.UNDISPATCHED) {
            editor.save {
                entered.complete(Unit)
                check(release.await(INTEREST_SAVE_DEADLINE_MS, TimeUnit.MILLISECONDS))
                assertEquals(listOf("first"), it.interests)
            }
        }
        try {
            withTimeout(INTEREST_SAVE_DEADLINE_MS) { entered.await() }
            editor.interests.value = "newer"
        } finally {
            release.countDown()
        }
        assertEquals(R.string.profile_saved, withTimeout(INTEREST_SAVE_DEADLINE_MS) { saving.await() })
        assertEquals("newer", editor.interests.value)
        assertTrue(editor.unsaved)
        editor.interests.value = "original"
        assertEquals(R.string.profile_saved, editor.save { assertEquals(listOf("original"), it.interests) })
        assertFalse(editor.unsaved)
    }
}
