package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.rules.RuleChain
import org.junit.Test

class SearchTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    // Waits until the search ends with one of the result texts or a failure text.
    private fun searched(): String {
        val loading = string(R.string.search_loading)
        compose.waitUntil(SEARCH_WAIT_MS) {
            compose.onAllNodes(hasTestTag("search-again")).fetchSemanticsNodes().isNotEmpty() &&
                compose.texts("search-status") != listOf(loading)
        }
        return compose.texts("search-status").single().orEmpty()
    }

    @Test
    fun searchWithoutAnActiveNetworkOffersRetry() {
        compose.skipNetworkEntry()
        click("search-open")
        compose.onNodeWithTag("search-status").assertTextEquals(string(R.string.search_idle))
        click("run-search")
        check(searched() == string(R.string.search_not_connected)) { "status ${compose.texts("search-status")}" }
        compose.onNodeWithTag("search-again").assertTextEquals(string(R.string.search_retry))
        click("search-back")
        compose.onNodeWithTag("search-open").assertExists()
    }

    @Test
    fun searchThroughTheLocalNodeShowsAVerifiedSample() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            click("search-open")
            click("run-search")
            checkLocalSample()
            compose.onNode(hasTestTag("search-again") and hasText(string(R.string.search_again))).assertExists()
        } finally {
            forgetNetworks()
        }
    }

    // Checks the local node answered with a sample or a coverage result, not a missing-node failure.
    private fun checkLocalSample() {
        val status = searched()
        val count = compose.onAllNodes(hasTestTag("search-result")).fetchSemanticsNodes().size
        val expected = listOf(string(R.string.search_empty),
            compose.activity.getString(R.string.search_results, count),
            compose.activity.getString(R.string.search_partial, count))
        val failures = compose.texts("search-failure")
        check(status in expected) { "status $status, failures $failures" }
        // The local node serves the search role by default.
        check(string(R.string.search_failure_no_nodes) !in failures) { "failures $failures" }
    }

    @Test
    fun offeredFiltersCheckTheAgeAndNarrowTheSample() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            click("search-open")
            click("run-search")
            checkLocalSample()
            compose.onNodeWithTag("search-age-min").performScrollTo().performTextInput("30")
            compose.onNodeWithTag("search-age-max").performScrollTo().performTextInput("20")
            click("run-search")
            check(searched() == string(R.string.search_age_invalid)) { "status ${compose.texts("search-status")}" }
            compose.onNodeWithTag("search-age-max").performScrollTo().performTextReplacement("60")
            click("profile-country")
            click("country-DE")
            click("run-search")
            checkLocalSample()
        } finally {
            forgetNetworks()
        }
    }

    // Run only by android-test-in-container.sh, after it flips the signature of every indexed record.
    @Test
    fun forgedResultsAreDroppedAndListed() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("forged") == "1")
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            click("search-open")
            click("run-search")
            check(searched() == string(R.string.search_empty)) { "status ${compose.texts("search-status")}" }
            val failures = compose.texts("search-failure")
            check(compose.onAllNodes(hasTestTag("search-result")).fetchSemanticsNodes().isEmpty())
            check(string(R.string.search_failure_invalid) in failures) { "failures $failures" }
        } finally {
            forgetNetworks()
        }
    }

    private companion object {
        const val SEARCH_WAIT_MS = 60_000L
    }
}
