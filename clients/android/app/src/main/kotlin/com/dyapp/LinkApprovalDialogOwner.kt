package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** UI opening ownership only; retiring a dialog does not cancel an admitted native call. */
internal class LinkApprovalDialogOwner(private val create: () -> LinkApprovalSession) {
    var current by mutableStateOf<LinkApprovalSession?>(null)
        private set
    private var active = true

    fun open() {
        if (active && current == null) current = create()
    }

    fun dismiss(expected: LinkApprovalSession) {
        if (!active || current !== expected || expected.state.value.operation == LinkApprovalOperation.SEND) return
        current = null
        expected.retire()
    }

    fun retire() {
        active = false
        current?.retire()
        current = null
    }
}
