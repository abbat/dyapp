package com.dyapp

import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileLinkReceiver
import uniffi.dyapp.MobileQrMatrix

/** Public display data supplied by core; Kotlin does not encode or parse the invitation. */
internal data class LinkReceiverDisplay(val code: String, val expiresAt: ULong, val matrix: MobileQrMatrix)

/** Blocking native calls, kept together with the exact handle that owns their receipt. */
internal class LinkReceiverCalls(
    val display: LinkReceiverDisplay,
    val phase: () -> MobileLinkPhase,
    val cancel: () -> MobileLinkPhase,
    val poll: () -> MobileLinkPhase,
    val receipt: () -> MobileLinkReceipt?,
    val close: () -> Unit
)

/** Capture the real selected registry; invitation and installation policy stay in Rust. */
internal fun linkReceiverCalls(opened: Opened.Ready, wizard: Boolean): LinkReceiverCalls {
    val receiver = MobileLinkReceiver.start(opened.slot.owner(), opened.slot.id, opened.peerId, wizard)
    return runCatching {
        LinkReceiverCalls(
            LinkReceiverDisplay(receiver.code(), receiver.expiresAt(), receiver.qrMatrix()),
            receiver::phase,
            receiver::cancel,
            receiver::poll,
            receiver::result,
            receiver::close
        )
    }.onFailure { receiver.close() }.getOrThrow()
}
