package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.runtime.key

/** Keep the selected message until admission; the native action captures it before dismissal. */
@Composable
internal fun ChatDeleteConfirm(form: ChatForm, actions: ChatActions) {
    val selected = form.deleting.value ?: return
    key(selected) {
        val cancel = { form.deleting.value = null }
        Confirm(
            "delete",
            R.string.delete_title,
            listOf(R.string.delete_text),
            R.string.chat_delete,
            cancel,
            admitted = { !form.busy && form.deleting.value == selected }
        ) {
            actions.onDelete()
            cancel()
        }
    }
}
