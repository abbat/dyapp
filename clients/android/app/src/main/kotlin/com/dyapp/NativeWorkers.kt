package com.dyapp

import uniffi.dyapp.ConnectionCheck
import uniffi.dyapp.ProfileSearcher
import uniffi.dyapp.ProfileSolve

/** Idle proof handle; each actual step retains its own verified core admission. */
internal fun Opened.Ready.profileSolver(): ProfileSolve =
    ProfileSolve.forSlot(slot.owner(), slot.id, peerId)

/** Idle search handle; use keyless methods without an external identity lease. */
internal fun Opened.Ready.profileSearcher(): ProfileSearcher =
    ProfileSearcher.forSlot(slot.owner(), slot.id, peerId)

/** Idle checker; run and activation reacquire core admission through actual completion. */
internal fun Opened.Ready.connectionChecker(): ConnectionCheck =
    ConnectionCheck.forSlot(slot.owner(), slot.id, peerId)
