package com.dyapp

internal enum class ReceivedViewsVisualState(val screen: String, val ready: String) {
    ROWS("settings-views", "received-view-open-1"),
    LOADING("settings-views-loading", "received-views-loading"),
    EMPTY("settings-views-empty", "received-views-empty"),
    ERROR("settings-views-error", "received-views-error"),
    STALE("settings-views-stale", "received-views-error")
}
