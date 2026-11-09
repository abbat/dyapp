package com.dyapp

import androidx.compose.runtime.Composable

/** The clear and block confirmations with their distinct effects. */
@Composable
internal fun ClearOrBlockConfirm(form: ChatForm, actions: ChatActions) {
    val cancel = { form.dialog.value = null }
    when (form.dialog.value) {
        ChatDialog.CLEAR -> Confirm("chat-clear", R.string.clear_title,
            listOf(R.string.clear_text, R.string.clear_later),
            R.string.clear_action, cancel) { cancel(); actions.onClear() }
        ChatDialog.BLOCK -> Confirm("block", R.string.block_title, listOf(R.string.block_text), R.string.chat_block,
            cancel) { cancel(); actions.onBlock() }
        null -> Unit
    }
}
