package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.profileScales

/** Rust scale keys shown in the first card; the others wait under `profile-more`. */
internal val BASIC_SCALES = setOf("gender", "relationship_goal")

// An open expander's arrow points up.
internal const val FLIPPED_DEGREES = 180f

private const val ABOUT_LINES = 3

/**
 * Profile draft editor and publication dialog over `client.db`; Rust supplies the closed wire choices.
 */
@Composable
internal fun ProfileDraftScreen(opened: Opened.Ready, persist: (ProfileDraft) -> Unit = { saveDraft(opened, it) }) {
    val scales = remember { profileScales() }
    val loaded by rememberNativeRead(opened) { loadDraft(opened) to loadVisibility(opened) }
    val snapshot = loaded?.getOrNull()
    when {
        loaded == null -> Text(stringResource(R.string.view_loading), Modifier.testTag("profile-loading"))
        snapshot == null -> Text(stringResource(R.string.profile_read_failed), Modifier.padding(24.dp))
        else -> key(opened) { ProfileDraftForm(opened, scales, snapshot, persist) }
    }
}

@Composable
private fun ProfileDraftForm(
    opened: Opened.Ready,
    scales: List<ProfileScale>,
    snapshot: Pair<ProfileDraft, VisibilityState>,
    persist: (ProfileDraft) -> Unit,
) {
    val (saved, visibility) = snapshot
    val lang = placeLang()
    val editorState = remember(opened) { mutableStateOf(ProfileEditor(saved, lang)) }
    val editor by editorState
    val (basics, more) = remember(scales) { scales.partition { it.key in BASIC_SCALES } }
    Column(
        Modifier.fillMaxSize().testTag("profile-editor").verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        HideCard(opened, visibility, editor.unsaved)
        key(editor) { PhotosCard(opened) }
        BasicsCard(basics, editor.values, editor.birth, editor.name)
        PlaceCard(editor.place)
        MoreScales(more, editor.values)
        SaveAndPublish(opened, visibility, editorState, persist)
    }
}

@Composable
private fun BasicsCard(
    scales: List<ProfileScale>,
    values: MutableMap<String, Int>,
    birth: BirthInputs,
    name: MutableState<String>,
) = FormCard {
    CardTitle(R.string.profile_basics)
    ProfileNameField(name)
    ScalePickers(scales, values)
    OutlinedTextField(
        birth.height,
        { birth.height = it },
        Modifier.fillMaxWidth().testTag("profile-height"),
        label = { Text(stringResource(R.string.profile_height)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        singleLine = true
    )
    OutlinedTextField(
        birth.about,
        { birth.about = it },
        Modifier.fillMaxWidth().testTag("profile-about"),
        label = { Text(stringResource(R.string.profile_about)) },
        supportingText = { Text(stringResource(R.string.profile_about_hint)) },
        minLines = ABOUT_LINES
    )
    CardTitle(R.string.profile_birth)
    BirthDateFields(birth)
    SeekingFields(scales, birth.seeking)
}

@Composable
private fun PlaceCard(place: PlaceInputs) = FormCard {
    CardTitle(R.string.profile_place_title)
    PlacePicker(place)
}

@Composable
private fun ScalePickers(scales: List<ProfileScale>, values: MutableMap<String, Int>) {
    for (scale in scales) {
        ScalePicker(scale, values[scale.key] ?: 0, R.string.profile_unset) { values[scale.key] = it }
    }
}

@Composable
private fun MoreScales(scales: List<ProfileScale>, values: MutableMap<String, Int>) {
    var open by rememberSaveable { mutableStateOf(false) }
    FormCard {
        TextButton(
            onClick = { open = !open },
            Modifier.fillMaxWidth().testTag("profile-more").semantics {
                if (open) {
                    collapse {
                        open = false
                        true
                    }
                } else {
                    expand {
                        open = true
                        true
                    }
                }
            }
        ) {
            Text(stringResource(R.string.profile_more), Modifier.weight(1f))
            Icon(painterResource(R.drawable.ic_expand_more), null, Modifier.rotate(if (open) FLIPPED_DEGREES else 0f))
        }
        if (open) ScalePickers(scales, values)
    }
}
