package com.dyapp

import android.content.res.Configuration
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.unit.width
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.Locale

class RenameDialogGeometryTests {
    @get:Rule
    val compose = createComposeRule()
    private val language = mutableStateOf("en")
    private val scale = mutableStateOf(1f)
    private val form = ChatForm(mutableStateOf(""))
    private var saves = 0

    private fun context() = InstrumentationRegistry.getInstrumentation().targetContext.let { base ->
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language.value))
        }
        base.createConfigurationContext(configuration)
    }

    private fun show() = compose.setContent {
        val localized = context()
        LocalizedTestContext(localized, scale.value) {
            MaterialTheme {
                if (form.renaming.value != null) RenameDialog(form) { saves++ }
            }
        }
    }

    @Test
    fun privateScopeCopyRemainsReadableAndActionsStayVisibleAtLargeText() {
        show()
        for (locale in listOf("en", "ru")) {
            for (fontScale in listOf(1f, 2f)) {
                checkCase(locale, fontScale)
            }
        }
        compose.runOnIdle { assertEquals(0, saves) }
        compose.runOnIdle { form.renaming.value = "" }
        compose.onNodeWithTag("chat-rename-save").assertIsDisplayed().performClick()
        compose.runOnIdle { assertEquals(1, saves) }
    }

    private fun checkCase(locale: String, fontScale: Float) {
        compose.runOnIdle {
            language.value = locale
            scale.value = fontScale
            form.renaming.value = "Robin"
        }
        assertCopy(R.string.rename_label)
        assertCopy(R.string.rename_hint)
        compose.onNodeWithTag("chat-rename-save").assertIsDisplayed()
        compose.onNodeWithTag("chat-rename-cancel").assertIsDisplayed().performClick()
        compose.onNodeWithTag("chat-rename-dialog").assertDoesNotExist()
    }

    private fun assertCopy(resource: Int) {
        val text = context().getString(resource)
        val node = compose.onNodeWithText(text, useUnmergedTree = true)
        node.performScrollTo().assertTextEquals(text)
        assertTrue("All private-scope copy must fit its text layout", node.textLayout().textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val dialog = compose.onNodeWithTag("chat-rename-dialog").getUnclippedBoundsInRoot()
        assertTrue("Current dialog must have width", dialog.width.value > 0f)
        assertTrue("Private-scope text stays in dialog", bounds.left >= dialog.left && bounds.right <= dialog.right)
    }
}
