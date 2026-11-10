package com.dyapp

import android.os.SystemClock

/** Advances completed operations until concrete state is ready; each await gets a fresh deadline. */
internal class PeerTransition(
    private val what: String,
    private val timeoutMs: Long = PEER_WAIT_MS,
    private val now: () -> Long = SystemClock::uptimeMillis,
) {
    fun await(ready: () -> Boolean, advance: () -> Unit) {
        val started = now()
        while (true) {
            checkDeadline(started)
            val complete = ready()
            checkDeadline(started)
            if (complete) return
            advance()
        }
    }

    private fun checkDeadline(started: Long) {
        check(now() - started < timeoutMs) { "Timed out waiting for $what after $timeoutMs ms" }
    }
}
