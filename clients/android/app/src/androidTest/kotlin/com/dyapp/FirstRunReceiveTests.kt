package com.dyapp

import android.content.Context
import android.content.ContextWrapper
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.test.espresso.Espresso.closeSoftKeyboard
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExternalResource
import org.junit.rules.RuleChain
import uniffi.dyapp.NetworkFile
import java.io.File

private class ReceivingContext(base: Context, private val root: File) : ContextWrapper(base) {
    override fun getApplicationContext(): Context = this
    override fun getNoBackupFilesDir(): File = root
}

class FirstRunReceiveTests {
    private val compose = createComposeRule()
    private lateinit var fixture: NativeSlotFixture
    private val owned = object : ExternalResource() {
        override fun before() { fixture = NativeSlotFixture() }
        override fun after() { fixture.close() }
    }

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(owned).around(compose)

    @Test
    fun explicitReceiveRestoresWithoutAnotherIdentityAndCancelPreservesTheDraft() {
        val context = ReceivingContext(InstrumentationRegistry.getInstrumentation().targetContext, fixture.root)
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            CompositionLocalProvider(LocalContext provides context) { MaterialTheme { MainScreen() } }
        }
        val receive = compose.awaitNode(hasTestTag("receive-identity"))
        assertTrue(fixture.slots.list().isEmpty())
        val oldReceive = receive.fetchSemanticsNode().config[applicationClickKey]
        receive.performScrollTo().performClick()
        compose.awaitNode(hasTestTag("network-save") and isEnabled())
        val original = fixture.slots.open() as Opened.Ready
        saveDraft(context)
        restoration.emulateSavedInstanceStateRestore()
        compose.awaitNode(hasTestTag("seeds") and hasText("127.0.0.1:33445"))
        compose.onNodeWithTag("skip-network").assertDoesNotExist()
        compose.runOnIdle { oldReceive() }
        compose.awaitNode(hasTestTag("cancel-receive-network")).performScrollTo().performClick()
        compose.awaitNode(hasTestTag("skip-network"))
        assertEquals(1, fixture.slots.list().size)
        val current = fixture.slots.open() as Opened.Ready
        assertEquals(original.peerId, current.peerId)
        assertEquals(listOf("127.0.0.1:33445"), RustBridge.loadNetwork(current, NetworkFile.DRAFT)?.seeds)
        assertNull(RustBridge.loadNetwork(current, NetworkFile.ACTIVE))
    }

    private fun saveDraft(context: Context) {
        compose.onNodeWithTag("seeds").performScrollTo().performTextReplacement("127.0.0.1:33445")
        closeSoftKeyboard()
        compose.onNodeWithTag("network-save").performScrollTo().performClick()
        compose.awaitNode(hasTestTag("network-status") and hasText(context.getString(R.string.network_saved)))
    }
}
