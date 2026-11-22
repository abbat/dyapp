package com.dyapp

import android.content.Context

/** Actual screen/configuration metadata shared by controlled native capture suites. */
internal data class NativeScreenVisualScene(
    val rootTag: String,
    val screen: String,
    val context: Context,
    val dark: Boolean,
    val fontScale: Float
)
