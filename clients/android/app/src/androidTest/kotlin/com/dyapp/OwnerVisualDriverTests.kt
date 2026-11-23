package com.dyapp

import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileLinkCodeStatus

/** Exercise the real enabled action and worker; no coordinates or backend state overwrite. */
class OwnerVisualDriverTests {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun sendRechecksExpiryAtLargeTextWithoutDelivering() {
        val scene = OwnerVisualScene(
            OwnerVisualBackend(OwnerVisualState.CONFIRM_EXPIRED),
            ownerVisualContext("en", 2f),
            false,
            2f
        )
        val host = OwnerVisualHost(compose)
        try {
            host.show(scene)
            OwnerVisualDriver(compose).show(scene.backend)
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
