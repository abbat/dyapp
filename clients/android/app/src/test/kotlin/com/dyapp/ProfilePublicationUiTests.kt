package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.dyapp.ProfilePublished

class ProfilePublicationUiTests {
    private fun result() = ProfilePublished(
        draft = ProfileDraft(name = "accepted").record(),
        version = 4uL,
        currentDraft = ProfileDraft(name = "persisted pending").record(),
        currentVersion = 5uL,
        hidden = true,
        pending = true,
    )

    @Test
    fun completionLoadsCurrentDraftWithoutSubstitutingItForTheReadback() {
        val editor = ProfileEditor(ProfileDraft(name = "before"), "en")
        val published = result()
        val current = editor.afterPublication(published, "en")
        assertNotSame(editor, current)
        assertEquals("persisted pending", current.name.value)
        assertFalse(current.unsaved)
        assertEquals("accepted", published.draft.name)
        assertTrue(published.pending)
    }

    @Test
    fun newerUnsavedInputsSurviveThePublicationCompletion() {
        val editor = ProfileEditor(ProfileDraft(name = "before"), "en")
        editor.name.value = "new typing"
        val current = editor.afterPublication(result(), "en")
        assertSame(editor, current)
        assertEquals("new typing", current.name.value)
        assertTrue(current.unsaved)
    }

    @Test
    fun completionCannotReplaceTheEditorOwnedByAnInFlightSave() {
        val editor = ProfileEditor(ProfileDraft(name = "being saved"), "en")
        assertSame(editor, editor.afterPublication(result(), "en", saving = true))
    }

    @Test
    fun actualCompletionKeepsNewTypingAndNewerVisibility() {
        var editor = ProfileEditor(ProfileDraft(name = "before"), "en")
        editor.name.value = "new typing"
        val visibility = VisibilityState(6uL, true)
        val state = PublishState(visibility) { editor = editor.afterPublication(it, "en") }
        state.open = true
        val published = result()
        state.completed(published)
        assertFalse(state.open)
        assertSame(published, state.published)
        assertEquals("new typing", editor.name.value)
        assertEquals(6uL, visibility.version)
        assertTrue(visibility.hidden)
    }

    @Test
    fun olderAndEqualVisibilityCompletionsCannotUndoAnAcknowledgedHide() {
        val visibility = VisibilityState(5uL, true)
        visibility.observed(4uL, false)
        visibility.observed(5uL, false)
        assertEquals(5uL, visibility.version)
        assertTrue(visibility.hidden)
        visibility.observed(6uL, false)
        assertEquals(6uL, visibility.version)
        assertFalse(visibility.hidden)
    }
}
