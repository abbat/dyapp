package com.dyapp

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileLinkCodeStatus

class LinkApprovalUITests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun uncertainSendKeepsTheOriginalTargetWhenExplicitRetryFindsExpiry() {
        val fixture = LinkApprovalUIFixture(compose)
        fixture.original.sending.set { error("later encrypted part refused") }
        try {
            fixture.show()
            fixture.enterCode()
            assertTargetAndWarnings(fixture)
            compose.onNodeWithTag("owner-send").performClick()
            compose.awaitNode(hasTestTag("owner-uncertain"))
                .assertTextEquals(fixture.text(R.string.owner_uncertain))
            compose.onNodeWithTag("owner-retry").assertIsEnabled()
            fixture.original.status.set(MobileLinkCodeStatus.EXPIRED)
            compose.onNodeWithTag("owner-retry").performClick()
            compose.awaitNode(hasTestTag("owner-expired"))
                .assertTextEquals(fixture.text(R.string.owner_code_expired))
            compose.onNodeWithTag("owner-uncertain").assertTextEquals(fixture.text(R.string.owner_uncertain))
            compose.onNodeWithTag("owner-recipient").assertTextEquals(fixture.original.normalized)
            compose.onNodeWithTag("owner-retry").assertDoesNotExist()
            assertEquals(1, fixture.original.sent.get())
        } finally {
            runBlocking { fixture.original.close() }
        }
    }

    @Test
    fun successfulSendRequiresOneExplicitConsentAndReturnsWithoutResending() {
        val fixture = LinkApprovalUIFixture(compose)
        try {
            fixture.show()
            fixture.enterCode()
            assertEquals(0, fixture.original.sent.get())
            val oldSend = fixture.callback("owner-send")
            compose.onNodeWithTag("owner-send").performClick()
            compose.awaitNode(hasTestTag("owner-sent")).assertTextEquals(fixture.text(R.string.owner_sent))
            compose.onNodeWithTag("owner-send").assertDoesNotExist()
            compose.runOnIdle { oldSend.invoke() }
            assertEquals(1, fixture.original.sent.get())
            compose.onNodeWithTag("owner-done").performClick()
            compose.awaitNode(hasTestTag("owner-open")).assertIsEnabled()
            compose.onNodeWithTag("owner-dialog").assertDoesNotExist()
            assertEquals(1, fixture.created.get())
        } finally {
            runBlocking { fixture.original.close() }
        }
    }

    @Test
    fun expiredCoreInputNeverReachesConsentOrTransfer() {
        val fixture = LinkApprovalUIFixture(compose)
        fixture.original.status.set(MobileLinkCodeStatus.EXPIRED)
        try {
            fixture.show()
            compose.awaitNode(hasTestTag("owner-open")).performClick()
            compose.onNodeWithTag("owner-code-next").performClick()
            compose.awaitNode(hasTestTag("owner-expired"))
            compose.onNodeWithTag("owner-recipient").assertDoesNotExist()
            compose.onNodeWithTag("owner-send").assertDoesNotExist()
            compose.onNodeWithTag("owner-cancel").assertIsEnabled()
            assertEquals(0, fixture.original.created.get())
            assertEquals(0, fixture.original.sent.get())
        } finally {
            runBlocking { fixture.original.close() }
        }
    }

    private fun assertTargetAndWarnings(fixture: LinkApprovalUIFixture) {
        compose.onNodeWithTag("owner-identity").assertTextEquals("original-owner")
        compose.onNodeWithTag("owner-recipient").assertTextEquals(fixture.original.normalized)
        compose.onNodeWithTag("owner-consequences").performScrollTo()
            .assertTextEquals(fixture.text(R.string.owner_confirm_text))
        compose.onNodeWithTag("owner-possession").performScrollTo()
            .assertTextEquals(fixture.text(R.string.owner_confirm_warn))
        compose.onNodeWithTag("owner-send").assertIsEnabled()
        compose.onNodeWithTag("owner-cancel").assertIsEnabled()
    }
}
