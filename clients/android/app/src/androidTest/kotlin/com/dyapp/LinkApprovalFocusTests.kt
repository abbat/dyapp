package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileLinkCodeStatus

class LinkApprovalFocusTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun completedValidationRestoresSafeFocusWithoutRenderingTheBusyState() {
        val fixture = LinkApprovalUIFixture(compose)
        fixture.original.status.set(MobileLinkCodeStatus.MALFORMED)
        try {
            fixture.show()
            compose.awaitNode(hasTestTag("owner-open")).performClick()
            compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
            compose.onNodeWithTag("owner-code-input").performTextInput("invalid code")
            compose.awaitNode(hasTestTag("owner-code-input") and isFocused())
            compose.runOnIdle {
                // This IO-only worker finishes before the UI can render PREPARE; no timing race.
                runBlocking {
                    withTimeout(5_000) { checkNotNull(fixture.original.session.prepare("invalid code")).await() }
                }
            }
            compose.awaitNode(hasTestTag("owner-malformed"))
            compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
            assertEquals(0, fixture.original.created.get())
            assertEquals(0, fixture.original.sent.get())
        } finally {
            runBlocking { fixture.original.close() }
        }
    }
}
