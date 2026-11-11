package com.dyapp

import android.os.Looper
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

private const val SAVE_DEADLINE_SECONDS = 60L

private class ProfileSaveGate(private val opened: Opened.Ready) {
    val entered = CompletableFuture<Unit>()
    val calls = AtomicInteger()
    private val released = CountDownLatch(1)

    fun persist(draft: ProfileDraft) {
        check(Looper.myLooper() != Looper.getMainLooper()) { "Profile persistence reached UI" }
        calls.incrementAndGet()
        entered.complete(Unit)
        check(
            released.await(SAVE_DEADLINE_SECONDS, TimeUnit.SECONDS)
        ) { "Profile persistence fixture was not released" }
        saveDraft(opened, draft)
    }

    fun release() = released.countDown()
}

class ProfileSaveOwnershipTests {
    @get:Rule
    val compose = createComposeRule()

    private fun type(name: String, age: String) {
        compose.onNodeWithTag("profile-name").performScrollTo().performTextReplacement(name)
        compose.onNodeWithTag("profile-seeking-age-from").performScrollTo().performTextReplacement(age)
    }

    private fun editable(tag: String): String =
        compose.onNodeWithTag(tag).performScrollTo().fetchSemanticsNode().config[SemanticsProperties.EditableText].text

    private fun assertInputs(name: String, age: String) {
        assertEquals(name, editable("profile-name"))
        assertEquals(age, editable("profile-seeking-age-from"))
    }

    private fun assertStored(opened: Opened.Ready, name: String, age: UInt) {
        val saved = loadDraft(opened)
        assertEquals(name, saved.name)
        assertEquals(age, saved.seeking.ageFrom)
    }

    private fun cancelDuringSave(editor: ProfileEditor, gate: ProfileSaveGate) {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val completed = AtomicInteger()
        val job = compose.runOnIdle {
            scope.launch(start = CoroutineStart.UNDISPATCHED) {
                editor.save(gate::persist)
                completed.incrementAndGet()
            }
        }
        try {
            gate.entered.get(SAVE_DEADLINE_SECONDS, TimeUnit.SECONDS)
            compose.runOnIdle { job.cancel() }
        } finally {
            gate.release()
            scope.cancel()
            runBlocking { withTimeout(60_000) { job.join() } }
        }
        assertEquals(0, completed.get())
    }

    @Test
    fun cancellationDoesNotPublishOrNormalizeAWriteThatActuallyFinishesLater() {
        NativeSlotFixture().use { fixture ->
            val opened = fixture.slots.create() as Opened.Ready
            val gate = ProfileSaveGate(opened)
            compose.setContent {}
            val editor = compose.runOnIdle {
                ProfileEditor(ProfileDraft(), "en").apply {
                    name.value = "first"
                    birth.seeking.ageFrom.value = "018"
                }
            }
            cancelDuringSave(editor, gate)
            compose.runOnIdle {
                assertEquals("018", editor.birth.seeking.ageFrom.value)
                assertTrue(editor.unsaved)
            }
            assertStored(opened, "first", 18u)
        }
    }

    @Test
    fun newerInputsSurviveAnOlderSaveAndAnUnchangedSaveNormalizesItsOwnBaseline() {
        NativeSlotFixture().use { fixture ->
            val opened = fixture.slots.create() as Opened.Ready
            val gate = ProfileSaveGate(opened)
            compose.setContent { ProfileDraftScreen(opened, gate::persist) }
            compose.awaitNode(hasTestTag("profile-editor"))
            try {
                type("first", "018")
                val previous = compose.beginProfileSave()
                gate.entered.get(SAVE_DEADLINE_SECONDS, TimeUnit.SECONDS)
                compose.onNodeWithTag("profile-save").assertIsNotEnabled()
                type("newer", "019")
                gate.release()
                compose.awaitProfileSave(previous)
                assertInputs("newer", "019")
                compose.onNodeWithTag("profile-publish").performScrollTo().assertIsNotEnabled()
                assertStored(opened, "first", 18u)
                compose.saveProfile()
                assertInputs("newer", "19")
                compose.onNodeWithTag("profile-publish").performScrollTo().assertIsEnabled()
                assertStored(opened, "newer", 19u)
                assertEquals(2, gate.calls.get())
            } finally {
                gate.release()
            }
        }
    }
}
