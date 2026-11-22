package com.dyapp

import android.content.ClipboardManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.text.font.FontFamily
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.dyapp.MobileLinkFailure
import uniffi.dyapp.MobileLinkReceipt

/** Actual receiving widgets; public invitation data is local core output, not transfer authorization. */
class ReceiveIdentifierTests {
    @get:Rule
    val compose = createComposeRule()
    private val states = listOf(
        ReceiveVisualState.WAITING,
        ReceiveVisualState.RECEIVED,
        ReceiveVisualState.DESTINATION
    )

    @Test
    fun receivingIdentifiersStayGroupedAndCopyExactRawValuesAtBothTextScales() {
        val host = ReceiveVisualHost(compose)
        withReceiveVisualFixture { fixture ->
            for (language in listOf("en", "ru")) verifyLanguage(host, fixture, language)
        }
    }

    private fun verifyLanguage(host: ReceiveVisualHost, fixture: ReceiveVisualFixture, language: String) {
        for (scale in listOf(1f, 2f)) {
            for (state in states) {
                val scene = ReceiveVisualScene(state, fixture, ownerVisualContext(language, scale), false, scale)
                try {
                    host.show(scene)
                    verifyScene(scene)
                } finally {
                    host.hide()
                }
            }
        }
    }

    private fun verifyScene(scene: ReceiveVisualScene) {
        checkField(scene, "link-receive-source", scene.fixture.sourcePeer, R.string.link_receive_source)
        when (scene.state) {
            ReceiveVisualState.WAITING -> {
                checkField(
                    scene,
                    "link-receive-code",
                    checkNotNull(scene.snapshot.display).code,
                    R.string.link_receive_code_label
                )
            }
            ReceiveVisualState.RECEIVED -> {
                val receipt = scene.snapshot.receipt as MobileLinkReceipt.Installed
                checkField(scene, "link-receive-peer", receipt.selection.key.peerId(), R.string.link_receive_peer_label)
            }
            ReceiveVisualState.DESTINATION -> {
                val receipt = scene.snapshot.receipt as MobileLinkReceipt.Failed
                val failure = receipt.failure as MobileLinkFailure.Destination
                checkField(scene, "link-receive-destination", failure.slotId, R.string.link_receive_destination_id)
            }
            else -> { error("Identifier fixture needs a receiving, received or destination-failure state") }
        }
    }

    private fun checkField(
        scene: ReceiveVisualScene,
        tag: String,
        raw: String,
        label: Int
    ) {
        compose.onNodeWithText(scene.context.getString(label)).assertExists()
        val node = compose.onNodeWithTag(tag).performScrollTo().assertIsDisplayed().assertTextEquals(raw)
        val layout = node.textLayout()
        val groups = layout.layoutInput.text.text.split(' ')
        assertEquals(raw, groups.joinToString(""))
        assertTrue(groups.dropLast(1).all { it.length == 8 })
        assertTrue(groups.last().length in 1..8)
        assertEquals(FontFamily.Monospace, layout.layoutInput.style.fontFamily)
        assertEquals(scene.fontScale, layout.layoutInput.density.fontScale)
        assertTrue(layout.textFitsLayout())
        val bounds = node.getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("settings-receive").getUnclippedBoundsInRoot()
        assertTrue(bounds.left >= viewport.left && bounds.right <= viewport.right)
        val copyTag = if (tag == "link-receive-code") "link-receive-copy" else "copy-$tag"
        compose.onNodeWithTag(copyTag).performScrollTo().performClick()
        compose.runOnIdle {
            val clipboard = scene.context.getSystemService(ClipboardManager::class.java)
            assertEquals(raw, clipboard.primaryClip?.getItemAt(0)?.text?.toString())
        }
    }
}
