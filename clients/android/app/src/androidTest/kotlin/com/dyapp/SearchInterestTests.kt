package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import com.dyapp.ui.theme.DYAppTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.CoreException
import uniffi.dyapp.SearchOp
import uniffi.dyapp.profileInterestLimits

class SearchInterestTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun nativeConditionsUseCanonicalCompleteTagAndNeverSplitPunctuation() {
        val filters = SearchFilters(emptyList())
        filters.offered = listOf(19u)
        val samples = listOf(
            " E\u0301COLE " to "\u00e9cole",
            " Tea, Coffee; Music " to "tea, coffee; music"
        )
        for ((typed, expected) in samples) {
            filters.interest = typed
            val condition = filters.conditions().single()
            assertEquals(19u, condition.field)
            assertEquals(SearchOp.EQ, condition.op)
            assertEquals(listOf(expected), condition.strings)
            assertTrue(condition.ints.isEmpty())
        }
    }

    @Test
    fun offeredInvalidInputRefusesWhileBlankOrHiddenInputAddsNothing() {
        val filters = SearchFilters(emptyList())
        filters.offered = listOf(19u)
        for (typed in listOf("music\nother", "x".repeat(profileInterestLimits().maxTagChars.toInt() + 1))) {
            filters.interest = typed
            val error = assertThrows(CoreException::class.java) { filters.conditions() }
            assertTrue(error.toString().contains("invalid search interest"))
        }
        filters.offered = emptyList()
        assertTrue(filters.conditions().isEmpty())
        filters.offered = listOf(19u)
        filters.interest = " \n\t "
        assertTrue(filters.conditions().isEmpty())
    }

    @Test
    fun offeredFieldChangesRetainRawInputWithoutApplyingHiddenConditions() {
        val filters = SearchFilters(emptyList())
        compose.setContent {
            DYAppTheme {
                Column(Modifier.verticalScroll(rememberScrollState())) { SearchFilterFields(filters) }
            }
        }
        compose.onNodeWithTag("search-interest").assertDoesNotExist()
        compose.runOnIdle { filters.offered = listOf(19u) }
        compose.onNodeWithTag("search-interest").performScrollTo().performTextReplacement(" MUSIC ")
        compose.runOnIdle {
            assertEquals(listOf("music"), filters.conditions().single().strings)
            filters.offered = emptyList()
        }
        compose.onNodeWithTag("search-interest").assertDoesNotExist()
        compose.runOnIdle {
            assertTrue(filters.conditions().isEmpty())
            filters.offered = listOf(19u)
        }
        val draft = compose.onNodeWithTag("search-interest").performScrollTo()
            .fetchSemanticsNode()
            .config[SemanticsProperties.EditableText].text
        assertEquals(" MUSIC ", draft)
    }
}
