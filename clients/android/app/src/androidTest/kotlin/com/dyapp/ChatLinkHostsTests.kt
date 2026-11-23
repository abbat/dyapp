package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertHasNoClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.util.Locale

/** Actual message rendering calls the native parser; no copied Kotlin URL policy or fake bridge. */
class ChatLinkHostsTests {
    @get:Rule
    val compose = createComposeRule()

    private val current = mutableStateOf(message("peer", ""))
    private val destinations = "safe.example, evil.example, xn--bcher-kva.example:8443"

    private fun show(language: String, fontScale: Float = 1f): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val localized = base.createConfigurationContext(configuration)
        compose.setContent {
            LocalizedTestContext(localized, fontScale) {
                DYAppTheme {
                    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                        ChatBubble(current.value, false, Modifier.fillMaxWidth())
                    }
                }
            }
        }
        return localized
    }

    private fun verifyDestinations(language: String, fontScale: Float = 1f) {
        val localized = show(language, fontScale)
        compose.runOnIdle {
            current.value = message(
                "peer",
                "http://safe.example/a https://safe.example@evil.example/b " +
                    "https://b\u00fccher.example:8443/x https://xn--bcher-kva.example:8443/repeat"
            ).copy(forwarded = true)
        }
        val caption = compose.onNodeWithTag("chat-link-hosts")
            .performScrollTo()
            .assertIsDisplayed()
            .assertTextEquals(localized.getString(R.string.chat_link_hosts, destinations))
            .assertHasNoClickAction()
        assertTrue("Host caption must wrap without clipping", caption.textLayout().textFitsLayout())
        compose.runOnIdle { current.value = current.value.copy(text = "https://changed.example", edited = true) }
        compose.onNodeWithTag("chat-link-hosts")
            .assertTextEquals(localized.getString(R.string.chat_link_hosts, "changed.example"))
    }

    @Test
    fun englishCaptionShowsActualIdnHostsPortsAndNotCredentials() = verifyDestinations("en")

    @Test
    fun russianCaptionShowsActualIdnHostsPortsAndNotCredentials() = verifyDestinations("ru")

    @Test
    fun englishCaptionWrapsAtLargeFont() = verifyDestinations("en", 2f)

    @Test
    fun russianCaptionWrapsAtLargeFont() = verifyDestinations("ru", 2f)

    @Test
    fun deletedAndNonWebMessagesHaveNoStaleHostCaption() {
        show("en")
        compose.runOnIdle { current.value = message("peer", "https://retained.example") }
        compose.onNodeWithTag("chat-link-hosts").assertExists()
        val hidden = listOf(
            current.value.copy(deleted = true),
            message("peer", "no links"),
            message("peer", "javascript:alert(1) ftp://files.example"),
            message("peer", "").copy(text = null)
        )
        for (next in hidden) {
            compose.runOnIdle { current.value = next }
            compose.onNodeWithTag("chat-link-hosts").assertDoesNotExist()
        }
    }
}
