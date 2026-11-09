package com.dyapp

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.profileScales

class SearchTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int) = compose.activity.getString(id)

    private fun click(tag: String) = compose.onNodeWithTag(tag).performScrollTo().performClick()

    // Waits until the search ends with one of the result texts or a failure text.
    private fun searched(): String {
        val waiting = listOf(string(R.string.search_loading), string(R.string.search_idle))
        compose.waitUntil(SEARCH_WAIT_MS) {
            compose.texts("search-status").singleOrNull()?.let { it !in waiting } == true
        }
        return compose.texts("search-status").single().orEmpty()
    }

    @Test
    fun searchWithoutAnActiveNetworkOffersRetry() {
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-search").performClick()
        compose.onNodeWithTag("search-status").assertTextEquals(string(R.string.search_idle))
        click("run-search")
        check(searched() == string(R.string.search_not_connected)) { "status ${compose.texts("search-status")}" }
        compose.onNodeWithTag("search-retry").assertTextEquals(string(R.string.search_retry))
        click("search-open-network")
        compose.onNodeWithTag("network-seeds").assertExists()
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("peer-id").assertExists()
    }

    @Test
    fun searchThroughTheLocalNodeShowsAVerifiedSample() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.onNodeWithTag("nav-search").performClick()
            click("run-search")
            checkLocalSample()
            // The empty state points at the filters instead of offering another sample.
            val again = compose.onNodeWithTag("search-again")
            if (compose.onAllNodes(hasTestTag("result-1")).fetchSemanticsNodes().isEmpty()) {
                again.assertDoesNotExist()
            } else {
                again.assertTextEquals(string(R.string.search_again))
            }
        } finally {
            forgetNetworks()
        }
    }

    // Checks the local node answered with a sample or a coverage result, not a missing-node failure.
    private fun checkLocalSample() {
        val status = searched()
        val count = generateSequence(1) { it + 1 }
            .takeWhile { compose.onAllNodes(hasTestTag("result-$it")).fetchSemanticsNodes().isNotEmpty() }
            .count()
        val expected = listOf(
            string(R.string.search_empty),
            compose.activity.getString(R.string.search_results, count),
            compose.activity.getString(R.string.search_partial, count)
        )
        val failures = compose.textsIn("search-failures")
        check(status in expected) { "status $status, failures $failures" }
        // The local node serves the search role by default.
        check(string(R.string.search_failure_no_nodes) !in failures) { "failures $failures" }
    }

    @Test
    fun offeredFiltersCheckTheAgeAndNarrowTheSample() {
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.onNodeWithTag("nav-search").performClick()
            click("run-search")
            checkLocalSample()
            compose.onNodeWithTag("search-age-min").performScrollTo().performTextInput("30")
            compose.onNodeWithTag("search-age-max").performScrollTo().performTextInput("20")
            click("run-search")
            check(searched() == string(R.string.search_age_invalid)) { "status ${compose.texts("search-status")}" }
            compose.onNodeWithTag("search-age-max").performScrollTo().performTextReplacement("60")
            compose.onNodeWithTag("search-height-min").performScrollTo().performTextInput("190")
            compose.onNodeWithTag("search-height-max").performScrollTo().performTextInput("170")
            click("run-search")
            check(searched() == string(R.string.search_height_invalid)) { "status ${compose.texts("search-status")}" }
            compose.onNodeWithTag("search-height-max").performScrollTo().performTextReplacement("200")
            scaleFiltersOpenFromTheExpander()
            click("search-country")
            click("country-DE")
            click("run-search")
            checkLocalSample()
            mutualSearchNeedsTheSavedGenderAndAge()
        } finally {
            forgetNetworks()
        }
    }

    private fun scaleFiltersOpenFromTheExpander() {
        click("search-more")
        for (scale in listOf("smoking", "drinking", "tattoos", "marital_status", "pets")) {
            compose.onNodeWithTag("profile-$scale").performScrollTo().assertExists()
        }
    }

    private fun pick(tag: String, choice: String) {
        click(tag)
        compose.onNodeWithTag("$tag-$choice").performClick()
    }

    // Without an exact age it says why rather than searching unfiltered; with both it runs.
    private fun mutualSearchNeedsTheSavedGenderAndAge() {
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("birth-year").performScrollTo().performTextClearance()
        click("profile-save")
        compose.onNodeWithTag("nav-search").performClick()
        click("search-mutual")
        click("run-search")
        check(searched() == string(R.string.search_mutual_unavailable)) { "status ${compose.texts("search-status")}" }
        compose.onNodeWithTag("search-retry").assertDoesNotExist()
        compose.onNodeWithTag("nav-profile").performClick()
        compose.onNodeWithTag("birth-year").performScrollTo().performTextInput("1990")
        pick("birth-month", "1")
        pick("birth-day", "1")
        val woman = profileScales().first { it.key == GENDER }.choices.first { it.key == "Woman" }.code
        pick("profile-gender", "$woman")
        click("profile-save")
        compose.onNodeWithTag("nav-search").performClick()
        click("run-search")
        checkLocalSample()
    }

    // Run only by android-test-in-container.sh, after it flips the signature of every indexed record.
    @Test
    fun forgedResultsAreDroppedAndListed() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("forged") == "1")
        compose.skipNetworkEntry()
        try {
            compose.activateLocalNode()
            compose.onNodeWithTag("nav-search").performClick()
            click("run-search")
            check(searched() == string(R.string.search_empty)) { "status ${compose.texts("search-status")}" }
            val failures = compose.textsIn("search-failures")
            compose.onNodeWithTag("result-1").assertDoesNotExist()
            check(string(R.string.search_failure_invalid) in failures) { "failures $failures" }
        } finally {
            forgetNetworks()
        }
    }

    private companion object {
        const val SEARCH_WAIT_MS = 60_000L
    }
}
