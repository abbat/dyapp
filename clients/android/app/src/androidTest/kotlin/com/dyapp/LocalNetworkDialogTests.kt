package com.dyapp

import android.annotation.SuppressLint
import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.test.filters.SdkSuppress
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExternalResource
import org.junit.rules.RuleChain
import uniffi.dyapp.NetworkFile

// Setup/restore requires commit acknowledgement; KTX edit returns Unit and cannot preserve it.
private fun SharedPreferences.Editor.commitChecked() = check(commit()) { "Permission fixture commit failed" }

/** Only the modern permission lane clears OS decision flags before this process starts. */
@SdkSuppress(minSdkVersion = 37)
class LocalNetworkDialogTests {
    val compose = createAndroidComposeRule<MainActivity>()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val preferences = context.getSharedPreferences("local-network-permission", Context.MODE_PRIVATE)
    private var previouslyRequested = false

    private val prompt = object : ExternalResource() {
        override fun before() {
            assumeTrue(InstrumentationRegistry.getArguments().getString("localNetworkForeground") == "true")
            check(context.applicationInfo.targetSdkVersion >= 37)
            previouslyRequested = preferences.getBoolean("auto-requested", false)
            // Open both prompts explicitly after the activity and native form are ready.
            preferences.edit().putBoolean("auto-requested", true).commitChecked()
        }

        override fun after() {
            preferences.edit().putBoolean("auto-requested", previouslyRequested).commitChecked()
        }
    }

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(prompt).around(lockChoice(skipped = true)).around(compose)

    @Test
    fun realDenialThenForegroundRetryKeepsTheOriginalIdentity() {
        PermissionDialog().use { dialog ->
            compose.skipNetworkEntry()
            val slots = RustBridge.slots(context)
            val original = slots.open() as Opened.Ready
            val active = RustBridge.loadNetwork(original, NetworkFile.ACTIVE)
            compose.onNodeWithTag("nav-network").performClick()
            checkWith("not an address", dialog::deny)
            assertPermission(granted = false)
            recreateDuringRequest(original, dialog)
            checkWith("another invalid address") {}
            assertPermission(granted = true)
            assertEquals(original.peerId, (slots.open() as Opened.Ready).peerId)
            assertEquals(active, RustBridge.loadNetwork(original, NetworkFile.ACTIVE))
            assertEquals(listOf("another invalid address"), RustBridge.loadNetwork(original, NetworkFile.DRAFT)?.seeds)
        }
    }

    private fun request(seed: String) {
        compose.awaitNode(hasTestTag("check-and-connect") and isEnabled())
        compose.onNodeWithTag("network-seeds").performScrollTo().performTextReplacement(seed)
        // Completed rows and the IME can move the button below the current viewport.
        compose.onNodeWithTag("check-and-connect").performScrollTo().assertIsDisplayed().performClick()
    }

    private fun recreateDuringRequest(original: Opened.Ready, dialog: PermissionDialog) {
        val activity = compose.activity
        val before = RustBridge.loadNetwork(original, NetworkFile.DRAFT)
        request("retired request must not run")
        dialog.awaitRequest()
        recreateWithPendingPermission(activity)
        dialog.allow()
        assertPermission(granted = true)
        compose.skipNetworkEntry()
        compose.onNodeWithTag("nav-network").performClick()
        compose.awaitNode(hasTestTag("check-and-connect") and isEnabled())
        compose.onNodeWithTag("network-check-status").assertDoesNotExist()
        assertEquals(before, RustBridge.loadNetwork(original, NetworkFile.DRAFT))
    }

    private fun checkWith(seed: String, answer: () -> Unit) {
        request(seed)
        answer()
        val invalid = context.getString(R.string.network_invalid_address)
        compose.awaitNode(hasTestTag("seed-rows") and hasAnyDescendant(hasText(seed)))
        compose.awaitNode(hasTestTag("network-check-status") and hasText(invalid))
    }

    @SuppressLint("InlinedApi") // SdkSuppress gates OS 37; setup also requires target API 37.
    private fun assertPermission(granted: Boolean) {
        val expected = if (granted) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED
        assertEquals(expected, context.checkSelfPermission(android.Manifest.permission.ACCESS_LOCAL_NETWORK))
    }
}
