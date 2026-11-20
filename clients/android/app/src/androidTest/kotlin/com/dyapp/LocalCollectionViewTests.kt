package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LocalCollectionEntry

class LocalCollectionViewTests {
    @get:Rule
    val compose = createComposeRule()

    private val owner = mutableStateOf("original")
    private val entries = mutableStateOf(
        (0..69).map { index ->
            LocalCollectionEntry("peer-${index.toString().padStart(3, '0')}-complete-target", "Robin")
        }
    )

    private fun show() = compose.setContent {
        MaterialTheme {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                LocalCollectionView(owner.value, entries.value, {}) { entry, _ ->
                    Text(entry.target, Modifier.testTag("collection-row"))
                }
            }
        }
    }

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    @Test
    fun pagesAreBoundedAndSearchFindsTheCompleteTargetBeyondTheFirstPage() {
        show()
        compose.awaitNode(hasText("peer-000-complete-target"))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(32)
        click("collection-next")
        compose.awaitNode(hasText("peer-032-complete-target"))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(32)
        click("collection-next")
        compose.awaitNode(hasText("peer-064-complete-target"))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(6)
        compose.onNodeWithTag("collection-query").performScrollTo().performTextReplacement("069-complete-target")
        compose.awaitNode(hasText("peer-069-complete-target"))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(1)
        compose.onNodeWithTag("collection-query").performTextReplacement("")
        compose.awaitNode(hasText("peer-000-complete-target"))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(32)
    }

    @Test
    fun retiredNavigationCannotChangeTheReplacementOpening() {
        show()
        compose.awaitNode(hasText("peer-000-complete-target"))
        val oldNext = checkNotNull(
            compose.onNodeWithTag("collection-next").fetchSemanticsNode()
                .config[SemanticsActions.OnClick].action
        )
        compose.runOnIdle {
            owner.value = "replacement"
            entries.value = listOf(LocalCollectionEntry("new-owner-target", "New"))
        }
        compose.awaitNode(hasText("new-owner-target"))
        compose.runOnIdle { oldNext.invoke() }
        compose.onNodeWithTag("collection-query").assert(hasText(""))
        compose.onAllNodesWithTag("collection-row").assertCountEquals(1)
        compose.awaitNode(hasText("new-owner-target"))
    }
}
