package com.dyapp

import androidx.compose.ui.test.junit4.v2.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.ProfileSearchResult

class SearchPresentationTests {
    @get:Rule
    val compose = createComposeRule()

    private val failure = SearchState.Failed(R.string.search_failed, "same failure")

    private fun done(field: UInt) = SearchState.Done(
        ProfileSearchResult(offered = listOf(field), profiles = emptyList(), partial = false, failures = emptyList())
    )

    @Test
    fun admissionPrecedesReadinessAndOldRunCannotRepeatAfterEqualFailure() {
        compose.setContent {}
        SearchRunFixture(compose).use { fixture ->
            val run = compose.runOnIdle { fixture.presentation.runAction() }
            compose.runOnIdle {
                run()
                run()
                fixture.filters.interest = "newer"
                fixture.presentation.runAction()()
                assertEquals(1, fixture.calls.size)
            }
            fixture.finish(fixture.calls.single(), failure)
            val admitted = fixture.calls.single().observed?.getOrThrow()
            assertEquals(listOf("original"), admitted?.choices?.single()?.strings)
            compose.runOnIdle {
                assertEquals(failure, fixture.presentation.state)
                run()
                assertEquals(1, fixture.calls.size)
                fixture.presentation.runAction()()
                assertEquals(2, fixture.calls.size)
            }
            fixture.finish(fixture.calls.last(), failure)
            compose.runOnIdle {
                run()
                assertEquals(2, fixture.calls.size)
            }
        }
    }

    @Test
    fun oldCancelAndLateCompletionCannotAlterASuccessorOrItsOfferedFields() {
        compose.setContent {}
        SearchRunFixture(compose).use { fixture ->
            val cancel = compose.runOnIdle {
                fixture.presentation.runAction()()
                fixture.presentation.cancelAction().also { it() }
            }
            compose.runOnIdle {
                fixture.filters.interest = "newer"
                fixture.presentation.runAction()()
                cancel()
                assertEquals(SearchState.Loading, fixture.presentation.state)
                assertEquals(2, fixture.calls.size)
            }
            val accepted = done(20u)
            fixture.finish(fixture.calls.last(), accepted)
            fixture.finish(fixture.calls.first(), done(19u))
            compose.runOnIdle {
                assertEquals(accepted, fixture.presentation.state)
                assertEquals(listOf(20u), fixture.filters.offered)
            }
        }
    }

    @Test
    fun retiredOwnerCannotInstallItsResultOrAdmitThroughRetainedCallbacks() {
        compose.setContent {}
        SearchRunFixture(compose).use { retired ->
            val run = compose.runOnIdle {
                retired.presentation.runAction().also { it() }
            }
            compose.runOnIdle { retired.presentation.close() }
            SearchRunFixture(compose).use { current ->
                compose.runOnIdle { current.presentation.runAction()() }
                current.finish(current.calls.single(), failure)
                retired.finish(retired.calls.single(), done(20u))
                compose.runOnIdle {
                    run()
                    assertEquals(1, retired.calls.size)
                    assertEquals(SearchState.Loading, retired.presentation.state)
                    assertEquals(listOf(19u), retired.filters.offered)
                    assertEquals(failure, current.presentation.state)
                    assertTrue(current.filters.interest == "original")
                }
            }
        }
    }
}
