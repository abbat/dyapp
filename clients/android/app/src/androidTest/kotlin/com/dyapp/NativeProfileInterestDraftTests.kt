package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Actual JNI record and storage semantics; UI text parsing has separate core tests. */
class NativeProfileInterestDraftTests {
    @Test
    fun optionalTagsRoundTripWithoutCollapsingPreserveAndClear() {
        for (tags in listOf(null, emptyList(), listOf("music", "tea, coffee", "music"))) {
            val draft = ProfileDraft(name = "Owner", interests = tags)
            assertEquals(tags, draft.record().interests)
            assertEquals(draft, draft.record().draft())
        }
        assertNull(ProfileDraft().record().interests)
    }

    @Test
    fun unrelatedSavePreservesTagsAndExplicitEmptyClearsAfterReopening() {
        NativeSlotFixture().use { fixture ->
            val owner = fixture.slots.create()
            val other = fixture.slots.create()
            val tags = listOf("music", "tea, coffee", "music")
            saveDraft(owner, ProfileDraft(name = "Before", interests = tags))
            saveDraft(other, ProfileDraft(name = "Other", interests = listOf("other")))
            fixture.slots.choose(other.slot.id)
            saveDraft(owner, ProfileDraft(name = "After"))
            val reopened = fixture.slots.choose(owner.slot.id) as Opened.Ready
            assertEquals(ProfileDraft(name = "After", interests = tags), loadDraft(reopened))
            saveDraft(reopened, ProfileDraft(name = "After", interests = emptyList()))
            val cleared = fixture.slots.choose(owner.slot.id) as Opened.Ready
            assertEquals(emptyList<String>(), loadDraft(cleared).interests)
            assertEquals(listOf("other"), loadDraft(other).interests)
        }
    }
}
