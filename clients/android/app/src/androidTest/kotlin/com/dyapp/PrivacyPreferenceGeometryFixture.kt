package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** Actual preference components and theme; controlled reads make every error/retry transition observable. */
internal class PrivacyPreferenceGeometryFixture(private val compose: ComposeContentTestRule) {
    val owner = mutableStateOf<Any>(Any())
    val scale = mutableStateOf(1f)
    val failReads = AtomicBoolean()
    val writes = AtomicInteger()
    private val calls = PrivacyPreferenceKind.entries.associateWith { kind ->
        PrivacyPreferenceCalls(read = {
            check(!failReads.get()) { "controlled read failure" }
            kind == PrivacyPreferenceKind.TYPING
        }, write = { writes.incrementAndGet() })
    }

    fun show(language: String): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent { Content(context) }
        return context
    }

    @Composable
    private fun Content(context: Context) {
        LocalizedTestContext(context, scale.value) {
            DYAppTheme { Controls() }
        }
    }

    @Composable
    private fun Controls() {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).testTag("privacy-geometry")) {
            PrivacyPreferenceKind.entries.forEach { kind ->
                PrivacyPreferenceSwitch(owner.value, kind, calls.getValue(kind))
            }
        }
    }
}
