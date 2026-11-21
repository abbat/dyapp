package com.dyapp

import androidx.compose.runtime.Composable
import uniffi.dyapp.ContactNameView

/** The clear and block confirmations with their distinct effects. */
@Composable
internal fun ClearOrBlockConfirm(form: ChatForm, actions: ChatActions, name: ContactNameView?) {
    val cancel = { form.dialog.value = null }
    when (form.dialog.value) {
        ChatDialog.CLEAR -> ChatClearDialog(form, name, actions.onClear)
        ChatDialog.BLOCK -> Confirm(
            "block",
            R.string.block_title,
            listOf(R.string.block_text),
            R.string.chat_block,
            cancel
        ) {
            cancel()
            actions.onBlock()
        }
        null -> Unit
    }
}
