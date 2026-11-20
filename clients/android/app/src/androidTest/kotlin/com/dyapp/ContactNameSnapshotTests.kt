package com.dyapp

import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ContactNameView

class ContactNameSnapshotTests {
    @get:Rule
    val compose = createComposeRule()

    private val owner = mutableStateOf("first")
    private val revision = mutableStateOf(0)
    private fun name(text: String) = ContactNameView("target", text, null, byteArrayOf())
    private fun label() = compose.onNodeWithTag("name")

    private fun show(
        first: suspend () -> List<ContactNameView>,
        second: suspend () -> List<ContactNameView> = first
    ) = compose.setContent {
        val selected = owner.value
        val read = if (selected == "first") first else second
        val names = rememberContactNameSnapshot(selected, revision.value, read)
        Text(names["target"]?.name.orEmpty(), Modifier.testTag("name"))
    }

    private fun await(signal: CompletableDeferred<Unit>) {
        compose.waitUntil(timeoutMillis = 10_000) { signal.isCompleted }
        compose.waitForIdle()
    }

    @Test
    fun aNewSelectionStartsEmptyWhileItsOwnReadIsPendingOrFails() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        show(
            { listOf(name("private first label")) },
            {
                started.complete(Unit)
                try {
                    release.await()
                    error("controlled replacement read failure")
                } finally {
                    returned.complete(Unit)
                }
            }
        )
        compose.awaitNode(hasText("private first label"))
        try {
            compose.runOnIdle { owner.value = "second" }
            await(started)
            label().assertTextEquals("")
        } finally {
            release.complete(Unit)
        }
        await(returned)
        label().assertTextEquals("")
    }

    @Test
    fun aRetiredNoncancellableReadCannotPopulateTheReplacement() {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val returned = CompletableDeferred<Unit>()
        show(
            {
                started.complete(Unit)
                try {
                    withContext(NonCancellable) { release.await() }
                    listOf(name("retired private label"))
                } finally {
                    returned.complete(Unit)
                }
            },
            { listOf(name("current label")) }
        )
        try {
            await(started)
            compose.runOnIdle { owner.value = "second" }
            compose.awaitNode(hasText("current label"))
        } finally {
            release.complete(Unit)
        }
        await(returned)
        label().assertTextEquals("current label")
    }

    @Test
    fun aFailedRefreshKeepsOnlyTheSameOwnersLastSuccessfulNames() {
        val failed = CompletableDeferred<Unit>()
        show(first = {
            if (revision.value > 0) {
                failed.complete(Unit)
                error("controlled refresh failure")
            }
            listOf(name("own last successful label"))
        })
        compose.awaitNode(hasText("own last successful label"))
        compose.runOnIdle { revision.value += 1 }
        await(failed)
        label().assertTextEquals("own last successful label")
    }
}
