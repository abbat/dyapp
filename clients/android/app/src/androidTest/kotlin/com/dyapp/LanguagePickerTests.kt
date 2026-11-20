package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.espresso.Espresso
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class LanguagePickerTests {
    @get:Rule
    val compose = createComposeRule()

    private val owner = mutableStateOf("first")
    private val selected = mutableStateOf<String?>(null)
    private val changes = mutableListOf<String?>()

    private fun show() = compose.setContent {
        MaterialTheme {
            LanguagePicker(owner.value, if (owner.value == "first") selected.value else "ru", languageFixture) {
                changes += it
                selected.value = it
            }
        }
    }

    private fun open(tag: String = "system") {
        compose.onNodeWithTag("language-choice").performClick()
        compose.awaitNode(hasTestTag("language-" + tag) and isFocused())
    }

    private fun action(tag: String): () -> Boolean {
        val node = compose.onNodeWithTag(tag).performScrollTo().fetchSemanticsNode()
        return checkNotNull(node.config[SemanticsActions.OnClick].action)
    }

    private fun closed() = compose.waitUntil(10_000) {
        compose.onAllNodesWithTag("language-picker").fetchSemanticsNodes().isEmpty()
    }

    @Test
    fun cancelAndAnOldOpeningCannotApplyChoicesToAReopenedMenu() {
        show()
        open()
        val retired = action("language-fr")
        compose.onNodeWithTag("language-picker-cancel").performClick()
        closed()
        compose.awaitNode(hasTestTag("language-choice") and isFocused())
        open()
        compose.runOnIdle { retired() }
        compose.onNodeWithTag("language-picker").assertExists()
        compose.onNodeWithTag("language-system").assertIsSelected()
        compose.onNodeWithTag("language-system").performClick()
        closed()
        compose.runOnIdle { assertEquals(emptyList<String?>(), changes) }
    }

    @Test
    fun nativeBackDismissesWithoutChangingThePreference() {
        show()
        open()
        Espresso.pressBack()
        closed()
        compose.awaitNode(hasTestTag("language-choice") and isFocused())
        compose.runOnIdle { assertEquals(emptyList<String?>(), changes) }
    }

    @Test
    fun allChoicesExistAndCapturedDoubleSelectionEmitsOneChange() {
        show()
        open()
        languageFixture.forEach {
            compose.onNodeWithTag("language-" + (it.tag ?: "system")).assertExists()
        }
        val choose = action("language-fr")
        compose.runOnIdle { repeat(2) { choose() } }
        closed()
        compose.runOnIdle { assertEquals(listOf("fr"), changes) }
    }

    @Test
    fun aRetiredOwnerCannotSelectOrCloseTheCurrentMenu() {
        show()
        open()
        val retiredChoice = action("language-fr")
        val retiredCancel = checkNotNull(
            compose.onNodeWithTag("language-picker-cancel").fetchSemanticsNode().config[SemanticsActions.OnClick].action
        )
        compose.runOnIdle { owner.value = "second" }
        closed()
        open("ru")
        compose.runOnIdle {
            retiredChoice()
            retiredCancel()
        }
        compose.onNodeWithTag("language-picker").assertExists()
        compose.onNodeWithTag("language-ru").assertIsSelected()
        compose.runOnIdle { assertEquals(emptyList<String?>(), changes) }
    }
}
