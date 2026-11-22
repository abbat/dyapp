package com.dyapp

import uniffi.dyapp.MobileLinkPhase

/** Exact receiving/setup neighbors already present in the Android prototype inventory. */
internal enum class ReceiveVisualState(
    val screen: String,
    val phase: MobileLinkPhase? = MobileLinkPhase.WAITING,
    val installed: Boolean = false
) {
    INTRO("link-receive-intro", null),
    WIZARD_INTRO("link-receive-wizard-intro", null),
    STARTING("link-receive-starting", null),
    WAITING("link-receive-waiting"),
    RECEIVING("link-receive-receiving", MobileLinkPhase.RECEIVING),
    INSTALLING("link-receive-installing", MobileLinkPhase.INSTALLING),
    CANCELLED("link-receive-cancelled", MobileLinkPhase.CANCELLED),
    EXPIRED("link-receive-expired", MobileLinkPhase.CANCELLED),
    INVALIDATED("link-receive-invalidated", MobileLinkPhase.INVALIDATED),
    ADMISSION("link-receive-failed-admission", MobileLinkPhase.FAILED),
    RESERVATION("link-receive-failed-reservation", MobileLinkPhase.FAILED),
    DESTINATION("link-receive-failed-destination", MobileLinkPhase.FAILED),
    UNCONFIRMED("link-receive-failed-unconfirmed", MobileLinkPhase.FAILED),
    RECEIVED("link-receive-received", MobileLinkPhase.COMPLETED, true),
    DURABILITY("link-receive-durability", MobileLinkPhase.COMPLETED, true),
    SETUP_RUNNING("link-setup-running", MobileLinkPhase.COMPLETED, true),
    COMPLETE("link-setup-complete", MobileLinkPhase.COMPLETED, true),
    REVIEW("link-setup-review", MobileLinkPhase.COMPLETED, true),
    PRIVATE_REVIEW("link-setup-private-review", MobileLinkPhase.COMPLETED, true),
    AUTHORIZATION("link-setup-authorization", MobileLinkPhase.COMPLETED, true),
    REVOKED("link-setup-revoked", MobileLinkPhase.COMPLETED, true),
    UNLISTED("link-setup-unlisted", MobileLinkPhase.COMPLETED, true),
    SETUP_ERROR("link-setup-error", MobileLinkPhase.COMPLETED, true),
    START_ERROR("link-receive-start-error", null),
    POLL_ERROR("link-receive-poll-error");

    val intro get() = this == INTRO || this == WIZARD_INTRO
}
