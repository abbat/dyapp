package com.dyapp

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertTrue
import uniffi.dyapp.MobileIdentityRegistry
import uniffi.dyapp.MobileSlotLayout

internal suspend fun awaitLinkReceiverIdle(receiver: LinkReceiverSession) {
    withTimeout(5_000) { receiver.state.first { !it.busy } }
}

/** Actual registry admission with controlled native-shaped receiving calls; no wire-transfer claim. */
internal suspend fun withInstalledLinkReceiver(block: suspend (LinkReceiverSession) -> Unit) {
    NativeSlotFixture().use { fixture ->
        MobileIdentityRegistry(
            fixture.root.absolutePath,
            MobileSlotLayout.ANDROID,
            NativeSlotKeys
        ).use { registry -> withInstalledSelection(registry, block) }
    }
}

private suspend fun withInstalledSelection(
    registry: MobileIdentityRegistry,
    block: suspend (LinkReceiverSession) -> Unit
) {
    val selection = registry.create()
    selection.key.use {
        val receiver = installedReceiver(selection)
        try {
            assertTrue(receiver.start())
            awaitLinkReceiverIdle(receiver)
            assertTrue(receiver.poll())
            awaitLinkReceiverIdle(receiver)
            block(receiver)
        } finally {
            awaitLinkReceiverIdle(receiver)
            receiver.close()
        }
    }
}
