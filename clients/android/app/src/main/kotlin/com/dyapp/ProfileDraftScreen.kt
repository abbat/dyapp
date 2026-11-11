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
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalResources
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

private val REPLACED_KEYS = BIRTH_KEYS + PLACE + HEIGHT
private const val ABOUT_LINES = 3

/**
 * Profile draft editor and publication dialog over `client.db`; Rust supplies the closed wire choices.
 */
@Composable
fun ProfileDraftScreen(opened: Opened.Ready) {
    val scales = remember { profileScales() }
    val loaded by rememberNativeRead(opened) { loadDraft(opened) to loadVisibility(opened) }
    val snapshot = loaded?.getOrNull()
    when {
        loaded == null -> Text(stringResource(R.string.view_loading), Modifier.testTag("profile-loading"))
        snapshot == null -> Text(stringResource(R.string.profile_read_failed), Modifier.padding(24.dp))
        else -> key(opened) { ProfileDraftForm(opened, scales, snapshot.first, snapshot.second) }
    }
}

private fun saveFailure(error: Throwable) = when {
    "invalidName" in error.toString() -> R.string.profile_name_invalid
    "invalidHeight" in error.toString() -> R.string.profile_height_invalid
    "invalidAbout" in error.toString() -> R.string.profile_about_invalid
    "invalidSeeking" in error.toString() -> R.string.profile_seeking_invalid
    else -> R.string.profile_save_failed
}

// [edited] holds the scales and the name; [birth] and [place] replace the values they hold.
private fun saveEdits(opened: Opened.Ready, edited: ProfileDraft, birth: BirthInputs, place: PlaceInputs): Int {
    val parts = parseBirthParts(birth.year, birth.month, birth.day)
    val tall = parsedHeight(birth.height)
    val seeking = birth.seeking.parsed()
    val picked = place.place?.let { mapOf(PLACE to it) }.orEmpty()
    val kept = edited.values.filterKeys { it !in REPLACED_KEYS }
    return when {
        parts == null -> R.string.profile_birth_invalid
        tall == null -> R.string.profile_height_invalid
        place.unknown -> R.string.profile_place_unknown
        seeking == null -> R.string.profile_seeking_invalid
        else -> runCatching {
            saveDraft(
                opened,
                edited.copy(
                    values = kept + parts + picked + tall,
                    country = place.country,
                    about = birth.about,
                    seeking = seeking
                )
            )
        }.fold({
            birth.seeking.showSaved(seeking)
            R.string.profile_saved
        }, ::saveFailure)
    }
}

@Composable
private fun ProfileDraftForm(
    opened: Opened.Ready,
    scales: List<ProfileScale>,
    saved: ProfileDraft,
    visibility: VisibilityState,
) {
    val values = remember(opened.slot.id) { mutableStateMapOf<String, Int>().apply { putAll(saved.values) } }
    val birth = remember(opened.slot.id) { BirthInputs(saved.values, saved.about, saved.seeking) }
    val name = remember(opened.slot.id) { mutableStateOf(saved.name) }
    val lang = placeLang()
    val place = remember(opened.slot.id) {
        PlaceInputs(saved.country, saved.values[PLACE], placeText(saved.country, saved.values[PLACE], lang))
    }
    val (basics, more) = remember(scales) { scales.partition { it.key in BASIC_SCALES } }
    val edits = inputs(values, birth, place) + name.value
    var savedEdits by remember(opened.slot.id) { mutableStateOf(edits) }
    val unsaved = edits != savedEdits
    Column(
        Modifier.fillMaxSize().testTag("profile-editor").verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        HideCard(opened, visibility, unsaved)
        PhotosCard(opened)
        BasicsCard(basics, values, birth, name)
        PlaceCard(place)
        MoreScales(more, values)
        SaveAndPublish(opened, visibility, unsaved) {
            saveEdits(opened, ProfileDraft(values, name = name.value), birth, place)
                .also { if (it == R.string.profile_saved) savedEdits = edits }
        }
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

private fun inputs(values: Map<String, Int>, birth: BirthInputs, place: PlaceInputs) =
    listOf(
        values.toMap(), birth.year, birth.month, birth.day, birth.height, birth.about, place.country, place.place,
        place.text, birth.seeking.tracked
    )

// Any edit or save recreates the publish section, which clears its confirmations and cancels an attempt.
@Composable
private fun SaveAndPublish(
    opened: Opened.Ready,
    visibility: VisibilityState,
    unsaved: Boolean,
    save: () -> Int,
) {
    val resources = LocalResources.current
    var status by remember { mutableStateOf("") }
    var saves by remember { mutableIntStateOf(0) }
    if (status.isNotEmpty()) Text(status, Modifier.testTag("profile-status"))
    key(saves, unsaved) {
        PublishSection(opened, visibility, unsaved) {
            OutlinedButton(onClick = {
                status = resources.getString(save())
                saves++
            }, modifier = Modifier.testTag("profile-save")) {
                Text(stringResource(R.string.profile_save))
            }
        }
    }
}
