package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class ChatMessageSelectionTests {
    @Test
    fun equalGroupAndIdFromDifferentAuthorsCannotAliasEitherSelection() {
        val first = message("first author", "first text")
        val second = first.copy(author = "second author", text = "second text")
        assertNotEquals(messageKey(first), messageKey(second))
        for (history in listOf(listOf(first, second), listOf(second, first))) {
            assertSame(first, history.byKey(messageKey(first)))
            assertSame(second, history.byKey(messageKey(second)))
        }
    }

    @Test
    fun completeSelectionIsStableAcrossFreshBindingRecordsAndSeparatesEveryKeyField() {
        val original = message("author", "text")
        val recreated = original.copy(groupId = original.groupId.copyOf(), id = original.id.copyOf())
        assertEquals(messageKey(original), messageKey(recreated))
        for (different in listOf(
            original.copy(groupId = byteArrayOf(9)),
            original.copy(author = "other author"),
            original.copy(id = byteArrayOf(9))
        )) {
            assertNotEquals(messageKey(original), messageKey(different))
        }
        assertSame(recreated, listOf(recreated).byKey(messageKey(original)))
    }

    @Test
    fun legacyIncompleteAndMissingSelectionsNeverPickAnAuthorByAccident() {
        val first = message("first author", "first")
        val second = first.copy(author = "second author", text = "second")
        val history = listOf(first, second)
        for (key in listOf(null, "", "00:01", "missing")) assertNull(history.byKey(key))
    }
}
