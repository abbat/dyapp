package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.StickerPackSaveControl
import uniffi.dyapp.StickerPackSaveState
import uniffi.dyapp.StickerPackSaveStatus
import java.util.Locale
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** Geometry only: typed snapshots are supplied; native action policy is tested separately. */
internal class StickerPackGeometryFixture(private val compose: ComposeContentTestRule) {
    val scale = mutableStateOf(1f)
    val failReads = AtomicBoolean()
    val writes = AtomicInteger()
    private val target = mutableStateOf<StickerPackTarget?>(null)
    private val name = "Pack <not markup> & long name ".repeat(3)
    private var checking = ""

    fun show(language: String): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        checking = context.getString(R.string.sticker_pack_checking, name)
        compose.setContent {
            LocalizedTestContext(context, scale.value) {
                DYAppTheme {
                    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).testTag("pack-geometry")) {
                        target.value?.let { StickerPackControls(it) {} }
                    }
                }
            }
        }
        return context
    }

    fun present(snapshot: StickerPackSaveStatus) {
        failReads.set(false)
        val calls = StickerPackCalls({
            check(!failReads.get()) { "controlled geometry readback failure" }
            snapshot
        }, { _, _ -> forbiddenWrite() })
        install(calls)
        awaitLoaded()
    }

    fun holdRead(): AutoCloseable {
        val response = CompletableFuture<StickerPackSaveStatus>()
        val loaded = StickerPackSaveStatus(
            StickerPackSaveState.NEW,
            null,
            null,
            null,
            listOf(StickerPackSaveControl.MANAGE)
        )
        install(StickerPackCalls({ response.get(60, TimeUnit.SECONDS) }, { _, _ -> forbiddenWrite() }))
        try {
            compose.awaitNode(hasTestTag("sticker-pack-REFRESH") and !isEnabled())
            return AutoCloseable {
                response.complete(loaded)
                awaitLoaded()
            }
        } catch (failure: Throwable) {
            response.complete(loaded)
            throw failure
        }
    }

    private fun install(calls: StickerPackCalls) = compose.runOnIdle {
        target.value = StickerPackTarget(Any(), name, calls)
    }

    private fun awaitLoaded() {
        compose.awaitNode(hasTestTag("sticker-pack-caption") and !hasText(checking))
        compose.awaitNode(hasTestTag("sticker-pack-MANAGE") and isEnabled())
    }

    private fun forbiddenWrite(): Nothing {
        writes.incrementAndGet()
        error("Geometry must not mutate sticker state")
    }
}
