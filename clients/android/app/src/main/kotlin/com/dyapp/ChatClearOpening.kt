package com.dyapp

import uniffi.dyapp.ChatClearTarget

/** UI-thread admission for one dialog; native Clear still revalidates under its lease. */
internal class ChatClearOpening(private val form: ChatForm) {
    private var active = true
    private var target: ChatClearTarget? = null

    fun show(value: ChatClearTarget?) {
        if (active) target = value
    }

    fun retry(): Boolean {
        val admitted = active && form.dialog.value == ChatDialog.CLEAR && !form.busy
        if (admitted) target = null
        return admitted
    }

    fun confirm(shown: ChatClearTarget?, action: () -> Unit) {
        if (current(shown) && !form.busy && form.dialog.value == ChatDialog.CLEAR) {
            active = false
            form.dialog.value = null
            action()
        }
    }

    fun cancel() {
        if (active && form.dialog.value == ChatDialog.CLEAR) {
            active = false
            form.dialog.value = null
        }
    }

    private fun current(shown: ChatClearTarget?): Boolean = active && shown != null && target === shown

    fun retire() {
        active = false
    }
}
