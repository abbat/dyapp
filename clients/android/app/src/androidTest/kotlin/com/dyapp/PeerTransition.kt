package com.dyapp

import android.os.SystemClock

/** Advances completed operations until concrete state is ready; each await gets a fresh deadline. */
internal class PeerTransition(
    private val what: String,
    private val timeoutMs: Long = PEER_WAIT_MS,
    private val now: () -> Long = SystemClock::uptimeMillis,
) {
    fun await(ready: () -> Boolean, advance: () -> Unit) = awaitSignal(ready) { advance() }

    fun awaitSignal(ready: () -> Boolean, advance: (Long) -> Unit) {
        val started = now()
        while (true) {
            remaining(started)
            val complete = ready()
            val budget = remaining(started)
            if (complete) return
            advance(budget)
        }
    }

    private fun remaining(started: Long): Long {
        val budget = timeoutMs - (now() - started)
        check(budget > 0) { "Timed out waiting for $what after $timeoutMs ms" }
        return budget
    }
}
