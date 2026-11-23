package com.dyapp

import android.app.Activity
import android.content.Intent
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.activity.result.ActivityResultRegistry
import androidx.activity.result.ActivityResultRegistryOwner
import androidx.activity.result.contract.ActivityResultContract
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.core.app.ActivityOptionsCompat
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileUpdateAvailability
import uniffi.dyapp.MobileUpdateDelivery
import uniffi.dyapp.MobileUpdateInterval
import uniffi.dyapp.MobileUpdateOffer
import uniffi.dyapp.MobileUpdateState

class UpdatePermissionResultTests {
    @get:Rule
    val compose = createComposeRule()

    private val registry = UpdatePermissionRegistry()
    private var owner by mutableStateOf(0)
    private var allowed = false
    private var reads = 0

    @Test
    fun actualRegistryCompletionRereadsPermissionInsteadOfTrustingTheCancelledResult() {
        show()
        val click = clickAction()
        compose.runOnIdle {
            click()
            click()
            assertEquals(1, registry.requests.size)
        }
        compose.onNodeWithTag("update-notice-allow").assertIsNotEnabled()
        compose.runOnIdle {
            allowed = true
            registry.dispatchResult(registry.requests.single(), Activity.RESULT_CANCELED, Intent())
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(
            hasTestTag("update-notice-permission") and hasText(context.getString(R.string.update_notice_allowed))
        )
    }

    @Test
    fun retiredRegistryResultAndCapturedActionCannotAffectAReplacementOpening() {
        show()
        val oldClick = clickAction()
        compose.runOnIdle { oldClick() }
        compose.runOnIdle { owner++ }
        compose.awaitNode(hasTestTag("update-notice-allow"))
        compose.runOnIdle {
            val before = reads
            oldClick()
            registry.dispatchResult(registry.requests.single(), Activity.RESULT_CANCELED, Intent())
            assertEquals(before, reads)
            assertEquals(1, registry.requests.size)
        }
        val currentClick = clickAction()
        compose.runOnIdle {
            currentClick()
            assertEquals(2, registry.requests.size)
        }
        compose.onNodeWithTag("update-notice-allow").assertIsNotEnabled()
    }

    private fun show() {
        compose.setContent {
            CompositionLocalProvider(LocalActivityResultRegistryOwner provides registry) {
                UpdateNoticePermission(owner, snapshot()) {
                    reads++
                    allowed
                }
            }
        }
        compose.awaitNode(hasTestTag("update-notice-allow"))
    }

    private fun clickAction(): () -> Boolean {
        val node = compose.onNodeWithTag("update-notice-allow").fetchSemanticsNode()
        return checkNotNull(node.config[SemanticsActions.OnClick].action)
    }

    private fun snapshot() = MobileUpdateState(
        installedVersion = "0.0.1",
        interval = MobileUpdateInterval.DAY,
        lastAttempt = 100uL,
        lastSuccess = 100uL,
        inFlight = false,
        availability = MobileUpdateAvailability.AVAILABLE,
        offer = MobileUpdateOffer(
            "0.0.2",
            MobileUpdateDelivery.PACKAGE,
            "https://github.com/abbat/dyapp/releases/tag/v0.0.2"
        ),
        notice = null,
    )
}

/** Launch capture controls the real registry completion without opening Android settings. */
internal class UpdatePermissionRegistry(private val failLaunch: Boolean = false) :
    ActivityResultRegistry(), ActivityResultRegistryOwner {
    val requests = mutableListOf<Int>()
    override val activityResultRegistry: ActivityResultRegistry get() = this

    override fun <I, O> onLaunch(
        requestCode: Int,
        contract: ActivityResultContract<I, O>,
        input: I,
        options: ActivityOptionsCompat?,
    ) {
        check(!failLaunch) { "Controlled unavailable system settings" }
        requests.add(requestCode)
    }
}
