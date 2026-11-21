package com.dyapp

/** UI intent only: core verifies whether the explicitly created wizard is still unused. */
internal data class LinkReceiveSource(val opened: Opened.Ready, val wizard: Boolean = false)
