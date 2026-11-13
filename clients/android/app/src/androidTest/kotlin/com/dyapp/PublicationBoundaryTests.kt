package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.test.junit4.v2.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ProfilePublished

class PublicationBoundaryTests {
    @get:Rule
    val compose = createComposeRule()

    private val owner = Any()
    private val revision = mutableStateOf(0)
    private val unsaved = mutableStateOf(false)
    private val saving = mutableStateOf(false)
    private val redraw = mutableStateOf(0)
    private val started = CompletableDeferred<Unit>()
    private val disposed = CompletableDeferred<Unit>()
    private val release = CompletableDeferred<Unit>()
    private var completions = 0
    private lateinit var publication: PublishState
    private val editor = mutableStateOf(ProfileEditor(ProfileDraft(name = "before"), "en"))
    private var accepted: ProfilePublished? = null

    private fun show() {
        compose.setContent {
            PublicationBoundary(Triple(owner, revision.value, "en"), unsaved.value, saving.value) {
                // Reading unrelated state forces recomposition without replacing the attempt.
                Text("${redraw.value}")
                Text(editor.value.name.value)
                publication = remember {
                    PublishState(VisibilityState(0uL, false)) { editor.value = editor.value.afterPublication(it, "en") }
                }
                LaunchedEffect(Unit) {
                    started.complete(Unit)
                    try {
                        release.await()
                        accepted?.let(publication::completed)
                        completions++
                    } finally {
                        disposed.complete(Unit)
                    }
                }
            }
        }
        compose.waitUntil { started.isCompleted }
        compose.runOnIdle {
            publication.adult = true
            publication.public = true
        }
    }

    private fun checkCancelled() = compose.runOnIdle {
        assertEquals(0, completions)
        assertFalse(publication.adult)
        assertFalse(publication.public)
    }

    @Test
    fun unrelatedRecompositionPreservesTheSuspendedAttempt() {
        show()
        compose.runOnIdle { redraw.value++ }
        compose.runOnIdle {
            assertFalse(disposed.isCompleted)
            assertTrue(publication.adult)
            assertTrue(publication.public)
            assertEquals(0, completions)
            release.complete(Unit)
        }
        compose.waitUntil { disposed.isCompleted }
        compose.runOnIdle { assertEquals(1, completions) }
    }

    @Test
    fun successfulCompletionKeepsExactReadbackAfterRefreshingTheEditor() {
        val result = ProfilePublished(
            draft = ProfileDraft(name = "accepted").record(),
            version = 4uL,
            currentDraft = ProfileDraft(name = "persisted pending").record(),
            currentVersion = 5uL,
            hidden = true,
            pending = true,
        )
        accepted = result
        show()
        lateinit var original: PublishState
        compose.runOnIdle {
            original = publication
            release.complete(Unit)
        }
        compose.waitUntil { disposed.isCompleted }
        compose.runOnIdle {
            assertSame(original, publication)
            assertSame(result, publication.published)
            assertEquals("accepted", publication.published?.draft?.name)
            assertEquals("persisted pending", editor.value.name.value)
            assertFalse(editor.value.unsaved)
            assertEquals(1, completions)
        }
    }

    @Test
    fun editingCancelsTheOldAttemptBeforeItCanComplete() {
        show()
        compose.runOnIdle { unsaved.value = true }
        compose.waitUntil { disposed.isCompleted }
        checkCancelled()
    }

    @Test
    fun startingASaveCancelsTheOldAttempt() {
        show()
        compose.runOnIdle { saving.value = true }
        compose.waitUntil { disposed.isCompleted }
        checkCancelled()
    }

    @Test
    fun aCompletedSaveRevisionCancelsTheOldAttempt() {
        show()
        compose.runOnIdle { revision.value++ }
        compose.waitUntil { disposed.isCompleted }
        checkCancelled()
    }
}
