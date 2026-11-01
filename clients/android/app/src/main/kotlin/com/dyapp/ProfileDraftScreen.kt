package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.profileScales

/** Rust scale keys shown in the first card; the others wait under `profile-more`. */
internal val BASIC_SCALES = setOf("gender", "relationship_goal")

private const val NO_PART = "-"

@Composable
internal fun label(context: Context, key: String): String {
    val name = "profile_" + key.replace('.', '_')
    val id = context.resources.getIdentifier(name, "string", context.packageName)
    return if (id == 0) key else stringResource(id)
}

/**
 * Profile draft editor and publication dialog over `client.db`; Rust supplies the closed wire choices.
 */
@Composable
fun ProfileDraftScreen(opened: Opened.Ready) {
    val scales = remember { profileScales() }
    // ponytail: Keystore unwrap and SQLite on the main thread, as identity opening; move off it
    // with the client runtime.
    val loaded = remember(opened.slot.id) { runCatching { loadDraft(opened, scales) } }
    if (loaded.isFailure) {
        Text(stringResource(R.string.profile_read_failed), Modifier.padding(24.dp))
        return
    }
    ProfileDraftForm(opened, scales, loaded.getOrThrow())
}

private class BirthInputs(saved: Map<String, Int>) {
    var year by mutableStateOf(saved[BIRTH_YEAR]?.toString().orEmpty())
    var month by mutableStateOf(saved[BIRTH_MONTH]?.toString().orEmpty())
    var day by mutableStateOf(saved[BIRTH_DAY]?.toString().orEmpty())
}

private fun saveEdits(opened: Opened.Ready, values: Map<String, Int>, birth: BirthInputs, place: PlaceInputs): Int {
    val parts = parseBirthParts(birth.year, birth.month, birth.day)
    val picked = place.place?.let { mapOf(PLACE to it) }.orEmpty()
    val kept = values.filterKeys { it !in BIRTH_KEYS && it != PLACE }
    return when {
        parts == null -> R.string.profile_birth_invalid
        place.unknown -> R.string.profile_place_unknown
        else -> runCatching { saveDraft(opened, ProfileDraft(kept + parts + picked, place.country)) }
            .fold({ R.string.profile_saved }, { R.string.profile_save_failed })
    }
}

@Composable
private fun ProfileDraftForm(
    opened: Opened.Ready,
    scales: List<ProfileScale>,
    saved: ProfileDraft,
) {
    val values = remember(opened.slot.id) { mutableStateMapOf<String, Int>().apply { putAll(saved.values) } }
    val birth = remember(opened.slot.id) { BirthInputs(saved.values) }
    val lang = placeLang()
    val place = remember(opened.slot.id) {
        PlaceInputs(saved.country, saved.values[PLACE], placeText(saved.country, saved.values[PLACE], lang))
    }
    val (basics, more) = remember(scales) { scales.partition { it.key in BASIC_SCALES } }
    val visibility = remember(opened.slot.id) { loadVisibility(opened) }
    val edits = inputs(values, birth, place)
    var savedEdits by remember(opened.slot.id) { mutableStateOf(edits) }
    val unsaved = edits != savedEdits
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, top = 8.dp, end = 16.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        HideCard(opened, visibility, unsaved)
        BasicsCard(basics, values, birth)
        PlaceCard(place)
        MoreScales(more, values)
        SaveAndPublish(opened, scales, visibility, unsaved) {
            saveEdits(opened, values, birth, place).also { if (it == R.string.profile_saved) savedEdits = edits }
        }
    }
}

@Composable
private fun BasicsCard(scales: List<ProfileScale>, values: MutableMap<String, Int>, birth: BirthInputs) = FormCard {
    CardTitle(R.string.profile_basics)
    ScalePickers(scales, values)
    CardTitle(R.string.profile_birth)
    BirthDateFields(birth)
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
                if (open) collapse { open = false; true } else expand { open = true; true }
            }
        ) {
            Text(stringResource(R.string.profile_more), Modifier.weight(1f))
            Icon(painterResource(R.drawable.ic_expand_more), null, Modifier.rotate(if (open) 180f else 0f))
        }
        if (open) ScalePickers(scales, values)
    }
}

private fun inputs(values: Map<String, Int>, birth: BirthInputs, place: PlaceInputs) =
    listOf(values.toMap(), birth.year, birth.month, birth.day, place.country, place.place, place.text)

// Any edit or save recreates the publish section, which clears its confirmations and cancels an attempt.
@Composable
@Suppress("LongParameterList") // The form's state and its save action, all from one caller.
private fun SaveAndPublish(
    opened: Opened.Ready,
    scales: List<ProfileScale>,
    visibility: VisibilityState,
    unsaved: Boolean,
    save: () -> Int,
) {
    val context = LocalContext.current
    var status by remember { mutableStateOf("") }
    var saves by remember { mutableIntStateOf(0) }
    if (status.isNotEmpty()) Text(status, Modifier.testTag("profile-status"))
    key(saves, unsaved) {
        PublishSection(opened, scales, visibility, unsaved) {
            OutlinedButton(onClick = {
                status = context.getString(save())
                saves++
            }, modifier = Modifier.testTag("profile-save")) {
                Text(stringResource(R.string.profile_save))
            }
        }
    }
}

@Composable
private fun BirthDateFields(birth: BirthInputs) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        val year = stringResource(R.string.profile_birth_part_year)
        Column(Modifier.weight(1.2f), Arrangement.spacedBy(4.dp)) {
            Caption(year)
            OutlinedTextField(birth.year, { birth.year = it }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.fillMaxWidth().testTag("birth-year").semantics { contentDescription = year })
        }
        BirthSelect(R.string.profile_birth_part_month, "birth-month", birth.month, 12) { birth.month = it }
        BirthSelect(R.string.profile_birth_part_day, "birth-day", birth.day, 31) { birth.day = it }
    }
    parseBirthParts(birth.year, birth.month, birth.day)?.let(::possibleAgeOnUtcToday)?.let {
        Caption(stringResource(R.string.profile_age_search, it), Modifier.testTag("profile-age"))
    }
}

@Composable
private fun RowScope.BirthSelect(name: Int, tag: String, value: String, last: Int, onPick: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column(Modifier.weight(1f), Arrangement.spacedBy(4.dp)) {
        Caption(stringResource(name))
        Box {
            // As tall as the year field beside it.
            OutlinedButton({ expanded = true }, Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag(tag),
                shape = MaterialTheme.shapes.extraSmall, contentPadding = PaddingValues(horizontal = 8.dp)) {
                Text(value.ifEmpty { NO_PART })
            }
            DropdownMenu(expanded, { expanded = false }) {
                DropdownMenuItem(text = { Text(NO_PART) }, onClick = { onPick(""); expanded = false },
                    modifier = Modifier.testTag("$tag-none"))
                for (part in 1..last) {
                    DropdownMenuItem(text = { Text(part.toString()) }, onClick = { onPick("$part"); expanded = false },
                        modifier = Modifier.testTag("$tag-$part"))
                }
            }
        }
    }
}

/** Picks one of the scale's choices or [none], shown with that label. */
@Composable
internal fun ScalePicker(scale: ProfileScale, selected: Int, none: Int, onSelect: (Int) -> Unit) {
    val context = LocalContext.current
    var expanded by remember { mutableStateOf(false) }
    // The prototype names this one control profile-goal; the key itself is a stored name.
    val tag = "profile-${scale.key.removePrefix("relationship_")}"
    Column(Modifier.fillMaxWidth(), Arrangement.spacedBy(4.dp)) {
        Caption(label(context, "field.${scale.key}"))
        Box {
            OutlinedButton(onClick = { expanded = true }, Modifier.fillMaxWidth().testTag(tag)) {
                val chosen = scale.choices.firstOrNull { it.code.toInt() == selected }
                val title = if (chosen == null) {
                    stringResource(none)
                } else {
                    label(context, "value.${scale.key}.${chosen.key}")
                }
                Text(title)
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                DropdownMenuItem(text = { Text(stringResource(none)) },
                    onClick = { onSelect(0); expanded = false })
                for (choice in scale.choices) {
                    DropdownMenuItem(text = { Text(label(context, "value.${scale.key}.${choice.key}")) },
                        onClick = { onSelect(choice.code.toInt()); expanded = false })
                }
            }
        }
    }
}
