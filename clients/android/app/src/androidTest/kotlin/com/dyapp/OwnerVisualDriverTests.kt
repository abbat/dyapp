package com.dyapp

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileLinkCodeStatus

/** Real large-text touch and worker expiry; no backend state overwrite. */
class OwnerVisualDriverTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun sendRechecksExpiryAtLargeTextWithoutDelivering() {
        val scene = OwnerVisualScene(
            OwnerVisualBackend(OwnerVisualState.CONFIRM),
            ownerVisualContext("en", 2f),
            false,
            2f
        )
        val host = OwnerVisualHost(compose)
        try {
            host.show(scene)
            OwnerVisualDriver(compose).show(scene.backend)
            scene.backend.inspect.set { MobileLinkCodeStatus.EXPIRED }
            compose.awaitNode(hasTestTag("owner-cancel") and isFocused())
            compose.awaitNode(hasTestTag("owner-send") and isEnabled()).performClick()
            compose.awaitSnapshot(scene.backend.approval.state, 5_000, "physical owner preflight") {
                !it.busy && it.codeStatus == MobileLinkCodeStatus.EXPIRED
            }
            assertEquals(MobileLinkCodeStatus.EXPIRED, scene.backend.approval.state.value.codeStatus)
            assertEquals(0, scene.backend.sent.get())
            compose.onNodeWithTag("owner-expired").assertExists()
            NativeVisualGeometry(compose, "owner-dialog").density(2f)
        } finally {
            try {
                host.hide(scene)
            } finally {
                scene.backend.close()
            }
        }
    }
}
