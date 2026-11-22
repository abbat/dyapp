package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.LinkReviewException
import java.util.concurrent.atomic.AtomicInteger

/** Production controls with typed boundary outcomes; no simulated authorization or transfer. */
class LinkEnrollmentNoticeTests {
    @get:Rule
    val compose = createComposeRule()

    private val state = mutableStateOf(LinkReceiverSnapshot())
    private val finishes = AtomicInteger()
    private val prepares = AtomicInteger()
    private val actions = LinkReceiveActions(
        {},
        {},
        {},
        {},
        {},
        LinkEnrollmentActions({ finishes.incrementAndGet() }, { prepares.incrementAndGet() }, { _, _ -> null })
    )

    @Test
    fun englishRefusalsRemainDistinctFromRetryableOutcomes() = checkLanguage("en")

    @Test
    fun russianRefusalsRemainDistinctFromRetryableOutcomes() = checkLanguage("ru")

    private fun checkLanguage(language: String) {
        val context = ownerVisualContext(language, 1f)
        compose.setContent {
            LocalizedTestContext(context, 1f) {
                MaterialTheme {
                    Column {
                        LinkEnrollmentNotice(state.value.enrollment)
                        LinkEnrollmentAction(state.value, actions.enrollment)
                    }
                }
            }
        }
        for ((failure, resource) in refusals()) {
            for (offered in listOf(false, true)) {
                show(failure, offered)
                compose.onNodeWithTag("link-setup-failure").assertTextEquals(context.getString(resource))
                absent("link-setup-finish")
                absent("link-setup-prepare")
            }
        }
        compose.runOnIdle {
            assertEquals(0, finishes.get())
            assertEquals(0, prepares.get())
        }
        checkPermittedActions()
    }

    private fun refusals() = listOf(
        LinkReviewException.EnrollmentAuthorizationRequired() to R.string.link_setup_authorization,
        LinkReviewException.PrivateStateReviewRequired() to R.string.link_setup_private_review,
        LinkReviewException.Revoked() to R.string.link_setup_revoked,
        LinkReviewException.DeviceNotListed() to R.string.link_setup_unlisted
    )

    private fun show(failure: Throwable, offered: Boolean = false) {
        compose.runOnIdle {
            val enrollment = LinkEnrollmentSnapshot(failure = failure, offered = offered)
            state.value = LinkReceiverSnapshot(enrollment = enrollment)
        }
        compose.awaitNode(hasTestTag("link-setup-failure"))
    }

    private fun absent(tag: String) = compose.onNodeWithTag(tag).assertDoesNotExist()

    private fun checkPermittedActions() {
        show(LinkReviewException.Failed("offline"))
        absent("link-setup-prepare")
        compose.awaitNode(hasTestTag("link-setup-finish")).performClick()
        compose.runOnIdle {
            assertEquals(1, finishes.get())
            assertEquals(0, prepares.get())
        }
        show(LinkReviewException.ReviewRequired())
        absent("link-setup-finish")
        compose.awaitNode(hasTestTag("link-setup-prepare")).performClick()
        compose.runOnIdle {
            assertEquals(1, finishes.get())
            assertEquals(1, prepares.get())
        }
    }
}
