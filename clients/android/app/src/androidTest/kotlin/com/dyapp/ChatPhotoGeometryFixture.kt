package com.dyapp

import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.core.graphics.createBitmap
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import uniffi.dyapp.ChatPhoto
import java.io.ByteArrayOutputStream
import java.util.Locale
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** A controlled byte loader exercises the real decoder, not core/network policy. */
internal class ChatPhotoGeometryFixture(private val compose: ComposeContentTestRule) {
    val scale = mutableStateOf(1f)
    val calls = AtomicInteger()
    val response = AtomicReference(CompletableFuture<ByteArray>())
    val photo = ChatPhoto(
        groupId = byteArrayOf(1), author = "peer", id = ByteArray(16),
        hlc = 1uL, sentAtMs = 0uL, size = 12uL, mime = "image/png",
        preview = byteArrayOf(), width = 40u, height = 20u,
        unread = true, forwarded = false, delivery = null,
    )

    fun show(language: String): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(base.resources.configuration).apply {
            setLocale(Locale.forLanguageTag(language))
        }
        val context = base.createConfigurationContext(configuration)
        compose.setContent {
            LocalizedTestContext(context, scale.value) {
                DYAppTheme {
                    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).testTag("photo-geometry")) {
                        key(scale.value) {
                            CompositionLocalProvider(
                                LocalChatPhotoAuto provides { true },
                                LocalChatPhotoLoader provides {
                                    calls.incrementAndGet()
                                    response.get().get(60, TimeUnit.SECONDS)
                                }
                            ) { ChatPhotoBubble(photo, stranger = true) }
                        }
                    }
                }
            }
        }
        return context
    }

    fun image(): ByteArray {
        val bitmap = createBitmap(40, 20, Bitmap.Config.ARGB_8888)
        return try {
            ByteArrayOutputStream().also {
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
            }.toByteArray()
        } finally {
            bitmap.recycle()
        }
    }
}
