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
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.core.app.ActivityOptionsCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
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
    private val lifecycleOwner = PermissionLifecycleOwner()
    private var owner by mutableStateOf(0)
    private var allowed = false
    private var queryFailure = false
    private var shown by mutableStateOf(true)
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

    @Test
    fun foregroundReadbackDoesNotReleaseAnOutstandingSystemLaunch() {
        show()
        val click = clickAction()
        compose.runOnIdle { click() }
        state(Lifecycle.State.STARTED)
        compose.runOnIdle { allowed = true }
        state(Lifecycle.State.RESUMED)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(
            hasTestTag("update-notice-permission") and hasText(context.getString(R.string.update_notice_allowed))
        )
        compose.onNodeWithTag("update-notice-allow").assertIsNotEnabled()
        compose.runOnIdle {
            click()
            assertEquals(1, registry.requests.size)
            registry.dispatchResult(registry.requests.single(), Activity.RESULT_CANCELED, Intent())
        }
        compose.onNodeWithTag("update-notice-allow").assertIsEnabled()
    }

    @Test
    fun failedResumeReadIsVisibleAndDisposedRowsStopObservingForeground() {
        show()
        state(Lifecycle.State.STARTED)
        compose.runOnIdle { queryFailure = true }
        state(Lifecycle.State.RESUMED)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.awaitNode(
            hasTestTag("update-notice-permission") and
                hasText(context.getString(R.string.update_notice_permission_failed))
        )
        compose.runOnIdle { shown = false }
        val before = compose.runOnIdle { reads }
        state(Lifecycle.State.STARTED)
        state(Lifecycle.State.RESUMED)
        compose.runOnIdle {
            assertEquals(before, reads)
            assertEquals(emptyList<Int>(), registry.requests)
        }
    }

    @Test
    fun retiredFailedReadActionCannotQueryTheReplacementOpening() {
        queryFailure = true
        show()
        val oldClick = clickAction()
        compose.runOnIdle { owner++ }
        compose.awaitNode(hasTestTag("update-notice-allow"))
        compose.runOnIdle {
            val before = reads
            oldClick()
            assertEquals(before, reads)
            assertEquals(emptyList<Int>(), registry.requests)
        }
    }

    private fun state(value: Lifecycle.State) =
        compose.runOnIdle { lifecycleOwner.registry.currentState = value }

    private fun show() {
        state(Lifecycle.State.RESUMED)
        compose.setContent {
            CompositionLocalProvider(
                LocalActivityResultRegistryOwner provides registry,
                LocalLifecycleOwner provides lifecycleOwner,
            ) {
                if (shown) {
                    UpdateNoticePermission(owner, snapshot()) {
                        reads++
                        check(!queryFailure) { "Controlled platform query failure" }
                        allowed
                    }
                }
            }
        }
        compose.awaitNode(hasTestTag("update-notice-allow"))
    }

    private fun clickAction(): () -> Unit {
        val node = compose.onNodeWithTag("update-notice-allow").fetchSemanticsNode()
        return node.config[applicationClickKey]
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

private class PermissionLifecycleOwner : LifecycleOwner {
    val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = registry
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
