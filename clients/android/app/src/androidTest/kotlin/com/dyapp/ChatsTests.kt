package com.dyapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExternalResource
import org.junit.rules.RuleChain

class ChatsTests {
    val compose = createComposeRule()
    private lateinit var fixture: NativeSlotFixture
    private val owned = object : ExternalResource() {
        override fun before() { fixture = NativeSlotFixture() }
        override fun after() { fixture.close() }
    }

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(owned).around(compose)

    private fun string(id: Int) = InstrumentationRegistry.getInstrumentation().targetContext.getString(id)

    @Test
    fun chatsTabOfANewIdentityIsEmptyAndHasNoBadge() {
        assertTrue(fixture.slots.list().isEmpty())
        compose.setContent {
            val parent = LocalContext.current
            val context = remember(parent) { fixture.context(parent) }
            CompositionLocalProvider(LocalContext provides context) { MaterialTheme { MainScreen() } }
        }
        compose.skipNetworkEntry()
        assertEquals(1, fixture.slots.list().size)
        val opened = fixture.slots.open() as Opened.Ready
        assertTrue(RustBridge.withChats(opened) { it.chatRows().isEmpty() })
        compose.awaitNode(hasTestTag("nav-chats")).performClick()
        compose.onNodeWithTag("app-ready").assertTextEquals(string(R.string.nav_chats))
        compose.onNodeWithTag("chats").assertExists()
        compose.onNodeWithText(string(R.string.chats_empty)).assertExists()
        compose.onNodeWithText(string(R.string.chats_identity)).assertExists()
        compose.onNodeWithTag("chats-unread", useUnmergedTree = true).assertDoesNotExist()
        // The first receive fails without a network; the app stays usable and the section still opens.
        compose.openProfileEditor()
        compose.onNodeWithTag("peer-id").assertTextEquals(opened.peerId)
    }
}
