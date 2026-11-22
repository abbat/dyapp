package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.unit.width
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class LanguagePickerGeometryTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun fullNamesWrapWithinTheCurrentDialogAtNormalAndEnlargedText() {
        val scale = mutableStateOf(1f)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.setContent {
            LocalizedTestContext(context, scale.value) {
                MaterialTheme { LanguagePicker("owner", null, languageFixture) {} }
            }
        }
        compose.onNodeWithTag("language-choice").performClick()
        compose.awaitNode(hasTestTag("language-system") and isFocused())
        for (value in listOf(1f, 2f)) {
            compose.runOnIdle { scale.value = value }
            assertFullName(value)
            compose.onNodeWithTag("language-picker-cancel").assertIsDisplayed()
        }
    }

    private fun assertFullName(scale: Float) {
        val node = compose.onNodeWithTag("language-label-fr", useUnmergedTree = true)
        node.performScrollTo().assertTextEquals(languageFixture.single { it.tag == "fr" }.name)
        val layout = node.textLayout()
        assertEquals("Actual language-label scale", scale, layout.layoutInput.density.fontScale, 0f)
        assertTrue("Long-name fixture must wrap at scale $scale", layout.lineCount > 1)
        assertTrue("All name characters must fit at scale $scale", layout.textFitsLayout())
        val dialog = compose.onNodeWithTag("language-picker").getUnclippedBoundsInRoot()
        val text = node.getUnclippedBoundsInRoot()
        assertTrue("The current dialog must have width", dialog.width.value > 0f)
        assertTrue("Label stays inside dialog", text.left >= dialog.left && text.right <= dialog.right)
    }
}
