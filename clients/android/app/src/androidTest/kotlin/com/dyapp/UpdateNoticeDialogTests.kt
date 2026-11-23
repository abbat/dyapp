package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

private fun string(id: Int, vararg args: Any) =
    InstrumentationRegistry.getInstrumentation().targetContext.getString(id, *args)

private fun offer(version: String = "0.0.2") = MobileUpdateOffer(
    version,
    MobileUpdateDelivery.PACKAGE,
    "https://github.com/abbat/dyapp/releases/tag/v$version",
)

private fun snapshot(notice: MobileUpdateOffer? = offer()) = MobileUpdateState(
    installedVersion = "0.0.1",
    interval = MobileUpdateInterval.DAY,
    lastAttempt = 1uL,
    lastSuccess = 1uL,
    inFlight = false,
    availability = MobileUpdateAvailability.AVAILABLE,
    offer = notice,
    notice = notice,
)

class UpdateNoticeDialogTests {
    @get:Rule
    val compose = createComposeRule()

    private fun showNotice(
        state: () -> UpdateNoticeUiState,
        later: (MobileUpdateOffer) -> Unit,
        open: (String) -> Unit,
    ) {
        compose.setContent { MaterialTheme { UpdateNoticeDialog(state(), later, open) } }
        compose.awaitNode(hasTestTag("update-notice"))
    }

    @Test
    fun opensOnlyTheCoreDestinationWithoutSilentlyDismissingTheNotice() {
        val opened = mutableListOf<String>()
        val dismissed = mutableListOf<MobileUpdateOffer>()
        compose.setContent {
            MaterialTheme { UpdateNoticeDialog(UpdateNoticeUiState(snapshot()), dismissed::add, opened::add) }
        }
        compose.awaitNode(hasTestTag("update-notice"))
        compose.onNodeWithText(string(R.string.update_installed, "0.0.1")).assertExists()
        compose.onNodeWithText(string(R.string.update_available_package, "0.0.2")).assertExists()
        val old = compose.onNodeWithTag("update-notice-open").fetchSemanticsNode().config[applicationClickKey]
        compose.onNodeWithTag("update-notice-open").performClick()
        compose.onNodeWithTag("update-notice-open").assertIsNotEnabled()
        compose.runOnIdle { old.invoke() }
        assertEquals(listOf(offer().releasePage), opened)
        assertEquals(emptyList<MobileUpdateOffer>(), dismissed)
        compose.onNodeWithTag("update-notice").assertExists()
    }

    @Test
    fun pendingWriteBlocksCallbacksAndFailureRetainsTheNoticeForExplicitRetry() {
        val shown = mutableStateOf(UpdateNoticeUiState(snapshot()))
        val dismissed = mutableListOf<MobileUpdateOffer>()
        val opened = mutableListOf<String>()
        showNotice({ shown.value }, dismissed::add, opened::add)
        val oldOpen = compose.onNodeWithTag("update-notice-open").fetchSemanticsNode().config[applicationClickKey]
        val oldLater = compose.onNodeWithTag("update-notice-later").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle { shown.value = shown.value.copy(pending = true) }
        compose.onNodeWithTag("update-notice-open").assertIsNotEnabled()
        compose.onNodeWithTag("update-notice-later").assertIsNotEnabled()
        compose.runOnIdle {
            oldOpen()
            oldLater()
        }
        assertEquals(emptyList<MobileUpdateOffer>(), dismissed)
        assertEquals(emptyList<String>(), opened)
        compose.runOnIdle { shown.value = shown.value.copy(pending = false, failed = true) }
        compose.onNodeWithTag("update-notice-status").assertTextEquals(string(R.string.update_notice_failed))
        compose.onNodeWithTag("update-notice-later").performClick()
        assertEquals(listOf(offer()), dismissed)
        compose.onNodeWithTag("update-notice").assertExists()
        compose.runOnIdle { shown.value = UpdateNoticeUiState(snapshot(null)) }
        compose.onNodeWithTag("update-notice").assertDoesNotExist()
    }

    @Test
    fun browserFailureCanBeRetriedAndReplacementRetiresBothOldActions() {
        val shown = mutableStateOf(UpdateNoticeUiState(snapshot()))
        val opened = mutableListOf<String>()
        val dismissed = mutableListOf<MobileUpdateOffer>()
        var fail = true
        showNotice({ shown.value }, dismissed::add) {
            if (fail) error("No browser")
            opened.add(it)
        }
        compose.onNodeWithTag("update-notice-open").performClick()
        compose.onNodeWithTag("update-notice-status").assertTextEquals(string(R.string.update_open_failed))
        val oldOpen = compose.onNodeWithTag("update-notice-open").fetchSemanticsNode().config[applicationClickKey]
        val oldLater = compose.onNodeWithTag("update-notice-later").fetchSemanticsNode().config[applicationClickKey]
        compose.runOnIdle {
            fail = false
            shown.value = UpdateNoticeUiState(snapshot(offer("0.0.3")))
        }
        compose.onNodeWithTag("update-notice-status").assertDoesNotExist()
        compose.runOnIdle {
            oldOpen()
            oldLater()
        }
        assertEquals(emptyList<String>(), opened)
        assertEquals(emptyList<MobileUpdateOffer>(), dismissed)
        compose.onNodeWithTag("update-notice-open").performClick()
        assertEquals(listOf(offer("0.0.3").releasePage), opened)
    }

    @Test
    fun aFailedBrowserHandoffCanRetryTheSameCoreDestination() {
        val opened = mutableListOf<String>()
        var fail = true
        compose.setContent {
            MaterialTheme {
                UpdateNoticeDialog(UpdateNoticeUiState(snapshot()), { error("Unexpected Later") }) {
                    if (fail) error("No browser")
                    opened.add(it)
                }
            }
        }
        compose.awaitNode(hasTestTag("update-notice"))
        compose.onNodeWithTag("update-notice-open").performClick()
        compose.onNodeWithTag("update-notice-status").assertExists()
        compose.runOnIdle { fail = false }
        compose.onNodeWithTag("update-notice-open").performClick()
        compose.onNodeWithTag("update-notice-status").assertDoesNotExist()
        compose.onNodeWithTag("update-notice-open").assertIsNotEnabled()
        assertEquals(listOf(offer().releasePage), opened)
    }

    @Test
    fun backUsesTheSameDurableLaterActionAndDisposalRetiresItsCallback() {
        val shown = mutableStateOf(UpdateNoticeUiState(snapshot()))
        val dismissed = mutableListOf<MobileUpdateOffer>()
        val opened = mutableListOf<String>()
        showNotice({ shown.value }, dismissed::add, opened::add)
        val old = compose.onNodeWithTag("update-notice-later").fetchSemanticsNode().config[applicationClickKey]
        Espresso.pressBack()
        compose.runOnIdle { assertEquals(listOf(offer()), dismissed) }
        compose.runOnIdle { shown.value = UpdateNoticeUiState(snapshot(null)) }
        compose.onNodeWithTag("update-notice").assertDoesNotExist()
        compose.runOnIdle { old() }
        assertEquals(listOf(offer()), dismissed)
        assertEquals(emptyList<String>(), opened)
    }

    @Test
    fun missingNoticeAndUnpinnedStoreHaveNoActionableDialog() {
        val shown = mutableStateOf(UpdateNoticeUiState(snapshot(null)))
        val opened = mutableListOf<String>()
        val dismissed = mutableListOf<MobileUpdateOffer>()
        compose.setContent {
            MaterialTheme { UpdateNoticeDialog(shown.value, dismissed::add, opened::add) }
        }
        compose.onNodeWithTag("update-notice").assertDoesNotExist()
        compose.runOnIdle {
            shown.value = UpdateNoticeUiState(snapshot(offer().copy(delivery = MobileUpdateDelivery.STORE)))
        }
        compose.onNodeWithTag("update-notice").assertDoesNotExist()
        assertEquals(emptyList<String>(), opened)
        assertEquals(emptyList<MobileUpdateOffer>(), dismissed)
    }
}
