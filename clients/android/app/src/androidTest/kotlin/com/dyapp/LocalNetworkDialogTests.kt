package com.dyapp

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExternalResource
import org.junit.rules.RuleChain
import uniffi.dyapp.NetworkFile

/** Only the modern permission lane clears OS decision flags before this process starts. */
@RequiresApi(37)
class LocalNetworkDialogTests {
    val compose = createAndroidComposeRule<MainActivity>()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val preferences = context.getSharedPreferences("local-network-permission", Context.MODE_PRIVATE)
    private var previouslyRequested = false

    private val prompt = object : ExternalResource() {
        override fun before() {
            assumeTrue(InstrumentationRegistry.getArguments().getString("localNetworkForeground") == "true")
            check(Build.VERSION.SDK_INT >= 37 && context.applicationInfo.targetSdkVersion >= 37)
            previouslyRequested = preferences.getBoolean("auto-requested", false)
            // Open both prompts explicitly after the activity and native form are ready.
            check(preferences.edit().putBoolean("auto-requested", true).commit())
        }

        override fun after() {
            check(preferences.edit().putBoolean("auto-requested", previouslyRequested).commit())
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
            checkWith("another invalid address", dialog::allow)
            assertPermission(granted = true)
            assertEquals(original.peerId, (slots.open() as Opened.Ready).peerId)
            assertEquals(active, RustBridge.loadNetwork(original, NetworkFile.ACTIVE))
            assertEquals(listOf("another invalid address"), RustBridge.loadNetwork(original, NetworkFile.DRAFT)?.seeds)
        }
    }

    private fun checkWith(seed: String, answer: () -> Unit) {
        compose.awaitNode(hasTestTag("check-and-connect") and isEnabled())
        compose.onNodeWithTag("network-seeds").performTextReplacement(seed)
        compose.onNodeWithTag("check-and-connect").performClick()
        answer()
        val invalid = context.getString(R.string.network_invalid_address)
        compose.awaitNode(hasTestTag("network-check-status") and hasText(invalid))
    }

    @SuppressLint("InlinedApi") // The outer rule requires both OS and target API 37.
    private fun assertPermission(granted: Boolean) {
        val expected = if (granted) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED
        assertEquals(expected, context.checkSelfPermission(android.Manifest.permission.ACCESS_LOCAL_NETWORK))
    }
}
