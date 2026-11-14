package com.dyapp

private const val LAN_PERMISSION_API = 37

/** Android 17 enforces LAN permission only after the app also targets API 37. */
internal fun localNetworkPermissionRequired(
    sdk: Int,
    target: Int
): Boolean = sdk >= LAN_PERMISSION_API && target >= LAN_PERMISSION_API

/** Permission completion releases foreground continuations even after denial: Internet still works. */
internal class NetworkPermissionRequests(private val missing: () -> Boolean) {
    private val waiting = mutableListOf<() -> Unit>()

    fun run(launch: () -> Unit, action: () -> Unit) {
        if (!missing()) {
            action()
            return
        }
        val first = waiting.isEmpty()
        waiting += action
        if (first) launch()
    }

    fun complete() {
        val actions = waiting.toList()
        waiting.clear()
        actions.forEach { it() }
    }

    /** Drop only operations not started yet; this never cancels an in-flight native operation. */
    fun clear() { waiting.clear() }
}
