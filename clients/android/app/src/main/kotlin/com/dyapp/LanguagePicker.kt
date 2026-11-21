package com.dyapp

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import kotlinx.coroutines.isActive

private class LanguagePickerPresentation {
    var opening by mutableStateOf<Any?>(null)
        private set
    var restoreFocus by mutableStateOf(false)

    fun open() { if (opening == null) opening = Any() }

    fun admit(token: Any): Boolean {
        if (opening !== token) return false
        opening = null
        return true
    }

    fun dismiss(token: Any) {
        if (admit(token)) restoreFocus = true
    }
}

/** Only the current opening can apply an immediate app-wide platform preference. */
@Composable
internal fun LanguagePicker(
    owner: Any,
    selected: String?,
    options: List<LanguageOption>,
    choose: (String?) -> Unit
) =
    key(owner) {
        val state = remember { LanguagePickerPresentation() }
        val scope = rememberCoroutineScope()
        val currentChoose by rememberUpdatedState(choose)
        Column(Modifier.testTag("language")) {
            Text(stringResource(R.string.settings_language))
            LanguageOpener(state, options.firstOrNull { it.tag == selected }?.name.orEmpty())
        }
        state.opening?.let { token ->
            LanguageDialog(options, selected, { if (scope.isActive) state.dismiss(token) }) { tag ->
                if (scope.isActive && state.admit(token)) {
                    if (tag != selected) currentChoose(tag) else state.restoreFocus = true
                }
            }
        }
    }

@Composable
private fun LanguageOpener(state: LanguagePickerPresentation, label: String) {
    val focus = remember { FocusRequester() }
    val ready = LocalWindowInfo.current.isWindowFocused
    LaunchedEffect(state.restoreFocus, ready) {
        if (state.restoreFocus && ready) {
            focus.requestFocus()
            state.restoreFocus = false
        }
    }
    TextButton(
        state::open,
        Modifier.fillMaxWidth().testTag("language-choice").focusRequester(focus).focusProperties { canFocus = true }
    ) { Text(label) }
}
