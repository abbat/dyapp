package com.dyapp

import android.content.Context
import android.content.res.Configuration
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.test.platform.app.InstrumentationRegistry
import com.dyapp.ui.theme.DYAppTheme
import kotlinx.coroutines.CompletableDeferred
import uniffi.dyapp.LinkReviewChoice
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSaved
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.Rgba
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Real form with controlled native results; never reimplements native selection or admission. */
internal class LinkReviewUiFixture(
    private val compose: ComposeContentTestRule,
    review: LinkReviewSnapshot = linkReviewFixture()
) {
    val scale = mutableFloatStateOf(1f)
    val snapshot = mutableStateOf(LinkReceiverSnapshot())
    var shown by mutableStateOf(true)
    var expectedPreview: LinkReviewProfile = review.local
    var previewFailure: Throwable? = null
    val previews = AtomicInteger()
    val saves = AtomicInteger()
    val choices = AtomicReference<List<LinkReviewChoice>>(emptyList())
    val confirmation = AtomicReference(CompletableDeferred<Result<LinkReviewSaved>>())
    val pixels = Rgba(2u, 1u, byteArrayOf(10, 20, 30, -1, 50, 60, 70, -1))
    val photo = AtomicReference(CompletableDeferred(Result.success(pixels)))
    val actions = LinkEnrollmentActions({}, {}, { owner, selected ->
        check(snapshot.value.enrollment.comparison === owner)
        saves.incrementAndGet()
        choices.set(selected)
        confirmation.get()
    })

    init { replace(review) }

    fun replace(review: LinkReviewSnapshot) {
        expectedPreview = review.local
        val opening = LinkReviewOpening(
            review,
            render = { owner, selected ->
                check(snapshot.value.enrollment.comparison === owner)
                previews.incrementAndGet()
                choices.set(selected)
                previewFailure?.let { throw it }
                expectedPreview
            },
            load = { owner, _, _ -> if (snapshot.value.enrollment.comparison === owner) photo.get() else null }
        )
        snapshot.value = LinkReceiverSnapshot(enrollment = LinkEnrollmentSnapshot(comparison = opening, offered = true))
    }

    fun show(language: String = "en", receivingFrame: Boolean = false): Context {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        val configuration = Configuration(
            base.resources.configuration
        ).apply { setLocale(Locale.forLanguageTag(language)) }
        val context = base.createConfigurationContext(configuration)
        compose.setContent {
            LocalizedTestContext(context, scale.floatValue) {
                DYAppTheme {
                    Frame(receivingFrame)
                }
            }
        }
        return context
    }

    @Composable
    private fun Frame(receiving: Boolean) {
        if (receiving) {
            SettingsFrame(remember { mutableStateOf(SettingsPane.RECEIVE) }) { Content() }
        } else {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).testTag("review-viewport")) {
                Content()
            }
        }
    }

    @Composable
    private fun Content() {
        if (shown) LinkReviewPanel(snapshot.value, actions) else Text("closed")
    }
}
