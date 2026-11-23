package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateState
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class UpdateMetadataCardTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun pendingReadDoesNotSelectADefaultAndCompletionShowsTheStoredInterval() {
        val entered = CompletableFuture<Unit>()
        val released = CountDownLatch(1)
        try {
            compose.setContent {
                MaterialTheme {
                    UpdateMetadataCard(Unit) {
                        entered.complete(Unit)
                        check(released.await(5, TimeUnit.SECONDS)) { "Update read was not released" }
                        snapshot()
                    }
                }
            }
            compose.waitUntil(5_000) { entered.isDone }
            compose.onNodeWithTag("update-24h").assertIsNotSelected()
            compose.onNodeWithTag("update-7d").assertIsNotSelected()
            compose.onNodeWithTag("update-metadata").assertExists()
            released.countDown()
            compose.awaitNode(hasTestTag("update-7d") and androidx.compose.ui.test.isSelected())
            compose.onNodeWithTag("update-7d").assertIsSelected()
        } finally {
            released.countDown()
        }
    }

    @Test
    fun storageFailureIsVisibleAndDoesNotSelectADefault() {
        compose.setContent {
            MaterialTheme { UpdateMetadataCard(Unit) { error("corrupt update record") } }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(hasTestTag("update-metadata") and hasText(context.getString(R.string.update_metadata_failed)))
        compose.onNodeWithTag("update-24h").assertIsNotSelected()
    }

    @Test
    fun outOfRangeTimestampIsNotWrappedIntoAnUnrelatedDate() {
        compose.setContent {
            MaterialTheme { UpdateMetadataCard(Unit) { snapshot().copy(lastSuccess = ULong.MAX_VALUE) } }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val expected = context.getString(
            R.string.update_last_check,
            context.getString(R.string.update_time_unavailable),
        )
        compose.awaitNode(hasTestTag("update-metadata") and hasText(expected))
    }

    private fun snapshot() = MobileUpdateState(
        installedVersion = "0.0.1",
        interval = MobileUpdateInterval.WEEK,
        lastAttempt = null,
        lastSuccess = null,
        inFlight = false,
        availability = MobileUpdateAvailability.NEVER_CHECKED,
        offer = null,
    )
}
