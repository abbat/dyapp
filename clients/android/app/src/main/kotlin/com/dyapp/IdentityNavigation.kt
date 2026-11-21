package com.dyapp

import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Job

/** Identity routes and exact-opening admission for the explicit first-run creation choice. */
internal class IdentityNavigation(
    start: StartTarget?,
    private val actions: IdentityActions,
    private val receiving: MutableState<ReceiveWizardIntent>,
    private val onLock: () -> Unit
) {
    var removing by mutableStateOf<Opened?>(null)
        private set
    var editor by mutableStateOf(start)
        private set
    val receiveIntent get() = receiving.value

    init {
        if (start != null) receiving.value = ReceiveWizardIntent()
    }

    fun completed(shown: Shown) {
        if (shown.opened !is Opened.Ready) editor = null
        if (receiveIntent.active && !receiveIntent.matches(shown.opened)) receiving.value = ReceiveWizardIntent()
    }

    fun createFromSetup(expected: Shown, create: () -> Opened.Ready, receive: Boolean): Job? {
        val state = actions.state.value
        if (state.shown !== expected || state.loading || expected.opened != Opened.Setup) return null
        return actions.perform(create) { shown ->
            completed(shown)
            if (receive) {
                (shown.opened as? Opened.Ready)?.let { receiving.value = ReceiveWizardIntent(it.slot.id, it.peerId) }
            }
        }
    }

    fun leaveReceiving(expected: ReceiveWizardIntent, action: (() -> Opened)? = null) {
        if (receiveIntent !== expected) return
        receiving.value = ReceiveWizardIntent()
        action?.let(::act)
    }

    fun act(action: () -> Opened) {
        dismiss()
        receiving.value = ReceiveWizardIntent()
        actions.perform(action, ::completed)
    }

    fun open(opened: Opened?, target: StartTarget?) {
        when (target) {
            StartTarget.REMOVE -> removing = opened
            StartTarget.LOCK -> onLock()
            else -> editor = target
        }
    }

    fun dismiss() { removing = null }
}
