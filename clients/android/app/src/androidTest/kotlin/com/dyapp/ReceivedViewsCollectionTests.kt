package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ReceivedProfileView
import java.util.concurrent.CopyOnWriteArrayList

/** Production collection and native pager, with explicit read completions rather than network timing. */
class ReceivedViewsCollectionTests {
    @get:Rule
    val compose = createComposeRule()

    private val owner = mutableStateOf("original")
    private val opened = mutableListOf<String>()
    private val pending = CopyOnWriteArrayList<CompletableDeferred<List<ReceivedProfileView>>>()
    private val rows = (0..69).map {
        ReceivedProfileView("peer-${it.toString().padStart(3, '0')}-complete-target", "2024-02-29")
    }

    private fun show() = compose.setContent {
        MaterialTheme {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                ReceivedViewsCollection(
                    owner.value,
                    {
                        CompletableDeferred<List<ReceivedProfileView>>().also { pending.add(it) }.await()
                    },
                    { opened.add(it) }
                )
            }
        }
    }

    private fun read(number: Int, values: List<ReceivedProfileView>) {
        compose.waitUntil(CHECK_WAIT_MS) { pending.size >= number }
        compose.runOnIdle { pending[number - 1].complete(values) }
    }

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    @Test
    fun loadingWaitsForTheReadAndSearchOpensTheExactTargetBeyondTheFirstPage() {
        show()
        compose.awaitNode(hasTestTag("received-views-loading"))
        compose.onNodeWithTag("collection-refresh").assertIsNotEnabled()
        read(1, rows)
        compose.awaitNode(hasText(rows.first().peerId))
        compose.onNodeWithTag("collection-query").performScrollTo()
            .performTextReplacement(rows.last().peerId)
        compose.awaitNode(hasText(rows.last().peerId))
        click("received-view-open-1")
        compose.runOnIdle { assertEquals(listOf(rows.last().peerId), opened) }
    }

    @Test
    fun aFailedRefreshKeepsRowsAndAnExplicitRetryCanShowAnEmptyCollection() {
        show()
        read(1, rows.take(1))
        compose.awaitNode(hasText(rows.first().peerId))
        click("collection-refresh")
        compose.waitUntil(CHECK_WAIT_MS) { pending.size == 2 }
        compose.onNodeWithTag("received-view-open-1").assertIsNotEnabled()
        compose.runOnIdle { pending[1].completeExceptionally(IllegalStateException("controlled read failure")) }
        compose.awaitNode(hasTestTag("received-views-error"))
        compose.awaitNode(hasText(rows.first().peerId))
        click("received-view-open-1")
        compose.runOnIdle { assertEquals(listOf(rows.first().peerId), opened) }
        click("collection-refresh")
        read(3, emptyList())
        compose.awaitNode(hasTestTag("received-views-empty"))
        compose.onNodeWithTag("received-views-error").assertDoesNotExist()
        compose.onNodeWithTag("received-view-open-1").assertDoesNotExist()
    }

    @Test
    fun anOldRowCallbackCannotOpenAProfileForTheReplacementOwner() {
        show()
        read(1, rows.take(1))
        compose.awaitNode(hasText(rows.first().peerId))
        val oldOpen = compose.onNodeWithTag("received-view-open-1")
            .fetchSemanticsNode()
            .config[applicationClickKey]
        compose.runOnIdle { owner.value = "replacement" }
        compose.awaitNode(hasTestTag("received-views-loading"))
        val fresh = ReceivedProfileView("replacement-owner-target", "2023-11-14")
        read(2, listOf(fresh))
        compose.awaitNode(hasText(fresh.peerId))
        compose.runOnIdle { oldOpen.invoke() }
        compose.runOnIdle { assertEquals(emptyList<String>(), opened) }
        click("received-view-open-1")
        compose.runOnIdle { assertEquals(listOf(fresh.peerId), opened) }
    }
}
