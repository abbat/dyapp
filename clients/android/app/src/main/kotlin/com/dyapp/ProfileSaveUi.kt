package com.dyapp

import android.content.res.Resources
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch

/** Actual completed saves; zero during a pending command, even if its previous text was identical. */
internal val profileSaveRevisionKey = SemanticsPropertyKey<Int>("ProfileSaveRevision")

private class ProfileSaveUi(private val scope: CoroutineScope, private val resources: Resources) {
    var status by mutableStateOf("")
        private set
    var revision by mutableIntStateOf(0)
        private set
    var saving by mutableStateOf(false)
        private set

    fun start(save: suspend () -> Int) {
        if (saving) return
        saving = true
        status = resources.getString(R.string.view_loading)
        // Freeze form inputs in this click before another input event can run.
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            try {
                status = resources.getString(save())
                revision++
            } finally {
                saving = false
            }
        }
    }
}

@Composable
private fun SaveStatus(state: ProfileSaveUi) {
    if (state.status.isNotEmpty()) {
        Text(
            state.status,
            Modifier.testTag("profile-status").semantics {
                set(profileSaveRevisionKey, if (state.saving) 0 else state.revision)
            }
        )
    }
}

// Editing or starting/completing a save clears the publication confirmations and cancels its attempt.
@Composable
internal fun SaveAndPublish(
    opened: Opened.Ready,
    visibility: VisibilityState,
    editor: MutableState<ProfileEditor>,
    persist: (ProfileDraft) -> Unit,
) {
    val resources = LocalResources.current
    val lang = placeLang()
    val scope = rememberCoroutineScope()
    val state = remember(scope, resources) { ProfileSaveUi(scope, resources) }
    SaveStatus(state)
    key(state, state.revision, editor.value.unsaved, state.saving, lang) {
        val publication = remember(visibility, editor, lang) {
            PublishState(
                visibility
            ) { result -> editor.value = editor.value.afterPublication(result, lang, state.saving) }
        }
        PublishSection(opened, publication, editor.value.unsaved || state.saving) {
            OutlinedButton(
                onClick = { state.start { editor.value.save(persist) } },
                modifier = Modifier.testTag("profile-save"),
                enabled = !state.saving
            ) {
                Text(stringResource(R.string.profile_save))
            }
        }
    }
}
