package com.dyapp

import uniffi.dyapp.NetworkSettings
import uniffi.dyapp.SeedRow
import uniffi.dyapp.setTestNodeIdPowBits
import uniffi.dyapp.setTestProfilePowBits

internal const val WORKER_ANCHOR = "worker anchor"
internal const val WORKER_OFFLINE = "worker offline"
internal const val WORKER_INCOMING = "worker incoming"

/** Every instrumentation process must match the real fixture node's proof policy. */
internal fun workerTestPolicy() {
    setTestNodeIdPowBits(8u)
    setTestProfilePowBits(4u)
}

internal fun publishWorkerPeer(ready: Opened.Ready) {
    saveDraft(
        ready,
        ProfileDraft(values = mapOf(GENDER to 1, BIRTH_YEAR to 1990, BIRTH_MONTH to 1, BIRTH_DAY to 1))
    )
    activateWorkerPeer(ready)
    ready.profileSolver().use { solver ->
        var solved = false
        PeerTransition("worker profile proof").await(
            ready = { solved },
            advance = { solved = solver.runStep(true).solved }
        )
    }
    check(!RustBridge.withChats(ready) { it.publishProfile(true) }.pending) {
        "Worker profile publication left a pending target"
    }
}

internal fun activateWorkerPeer(ready: Opened.Ready) = ready.connectionChecker().use { checker ->
    var active = false
    PeerTransition("worker node activation").await(
        ready = { active },
        advance = {
            val rows = checker.run(NetworkSettings(listOf(LOCAL_NODE), ""), CHECK_WAIT_MS.toUInt())
            check(rows.all { it is SeedRow.Connected || it is SeedRow.Unreachable }) {
                "Worker node configuration was rejected: $rows"
            }
            if (rows.any { it is SeedRow.Connected }) {
                checker.activate()
                active = true
            }
        }
    )
}

internal fun workerMessagePresent(ready: Opened.Ready, peer: String, id: ByteArray, text: String): Boolean {
    val matching = RustBridge.withChats(ready) { it.chatHistory(peer).messages }.filter { it.id.contentEquals(id) }
    check(matching.size <= 1) { "Duplicate worker message ID" }
    return matching.singleOrNull()?.text == text
}

/** Only the independent counterpart receives here; worker acceptance never fetches on its behalf. */
internal fun receiveWorkerCounterpart(ready: Opened.Ready, peer: String, id: ByteArray, text: String) {
    PeerTransition("counterpart receipt of $text").await(
        ready = { workerMessagePresent(ready, peer, id, text) },
        advance = { RustBridge.withChats(ready) { it.receive() } }
    )
}
