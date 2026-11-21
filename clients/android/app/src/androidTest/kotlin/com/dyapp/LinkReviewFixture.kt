package com.dyapp

import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.LinkReviewSnapshot
import uniffi.dyapp.MobileLinkPhase
import uniffi.dyapp.MobileLinkReceipt
import uniffi.dyapp.MobileQrMatrix
import uniffi.dyapp.MobileSlotSelection
import java.util.concurrent.atomic.AtomicReference

internal fun linkReviewFixture(): LinkReviewSnapshot {
    val local = LinkReviewProfile(
        version = 1uL,
        fields = ProfileDraft(name = "local").record(),
        incomeFrom = null, incomeTo = null, hasKids = null, wantsMoreKids = null,
        interests = emptyList(), photos = emptyList(), educationLevel = 0u,
        profession = "", company = "", industry = "", careerStage = "", workLifeBalance = ""
    )
    val remote = local.copy(fields = ProfileDraft(name = "remote").record())
    return LinkReviewSnapshot(local, remote, listOf(LinkReviewField.NAME), hidden = true)
}

/** Real native selection, controlled receipt: tests worker ownership, not wire installation. */
internal fun installedReceiver(selection: MobileSlotSelection): LinkReceiverSession {
    val phase = AtomicReference(MobileLinkPhase.WAITING)
    return LinkReceiverSession {
        LinkReceiverCalls(
            LinkReceiverDisplay("controlled-public-code", 123uL, MobileQrMatrix(1, listOf(true))),
            { phase.get() },
            { phase.get() },
            {
                phase.set(MobileLinkPhase.COMPLETED)
                phase.get()
            },
            { MobileLinkReceipt.Installed(selection) },
            {}
        )
    }
}
