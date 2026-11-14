package com.dyapp

/** A stopped platform task may still have an admitted Rust call finishing on its worker. */
internal class JobCompletion {
    private var ended = false

    @Synchronized
    fun isActive(): Boolean = !ended

    @Synchronized
    fun stop() {
        ended = true
    }

    @Synchronized
    fun complete(action: () -> Unit) {
        if (ended) return
        ended = true
        action()
    }
}
