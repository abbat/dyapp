package com.dyapp

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.profileScales
import java.io.File

@Composable
internal fun label(context: Context, key: String): String {
    val name = "profile_" + key.replace('.', '_')
    val id = context.resources.getIdentifier(name, "string", context.packageName)
    return if (id == 0) key else stringResource(id)
}

/** Local profile draft editor and preview; Rust supplies the closed wire choices. Publishing is not wired here. */
@Composable
fun ProfileDraftScreen(slot: Slot, onBack: () -> Unit) {
    val context = LocalContext.current
    val file = remember(slot.id) { File(slot.dir, "profile-draft.json") }
    val scales = remember { profileScales() }
    val loaded = remember(file) { runCatching { readDraft(file, scales) } }
    if (loaded.isFailure) {
        Column(Modifier.fillMaxSize().padding(24.dp)) {
            Text(stringResource(R.string.profile_read_failed))
            OutlinedButton(onClick = onBack) { Text(stringResource(R.string.profile_back)) }
        }
        return
    }
    ProfileDraftForm(file, scales, loaded.getOrThrow(), onBack)
}

private class BirthInputs(saved: Map<String, Int>) {
    var year by mutableStateOf(saved[BIRTH_YEAR]?.toString().orEmpty())
    var month by mutableStateOf(saved[BIRTH_MONTH]?.toString().orEmpty())
    var day by mutableStateOf(saved[BIRTH_DAY]?.toString().orEmpty())
}

private fun saveDraft(file: File, values: Map<String, Int>, birth: BirthInputs, place: PlaceInputs): Int {
    val parts = parseBirthParts(birth.year, birth.month, birth.day)
    val picked = place.place?.let { mapOf(PLACE to it) }.orEmpty()
    val kept = values.filterKeys { it !in BIRTH_KEYS && it != PLACE }
    return when {
        parts == null -> R.string.profile_birth_invalid
        place.unknown -> R.string.profile_place_unknown
        else -> runCatching { writeDraft(file, ProfileDraft(kept + parts + picked, place.country)) }
            .fold({ R.string.profile_saved }, { R.string.profile_save_failed })
    }
}

@Composable
private fun ProfileDraftForm(file: File, scales: List<ProfileScale>, saved: ProfileDraft, onBack: () -> Unit) {
    val context = LocalContext.current
    val values = remember(file) { mutableStateMapOf<String, Int>().apply { putAll(saved.values) } }
    var status by remember { mutableStateOf("") }
    var saves by remember { mutableIntStateOf(0) }
    val birth = remember(file) { BirthInputs(saved.values) }
    val lang = placeLang()
    val place = remember(file) {
        PlaceInputs(saved.country, saved.values[PLACE], placeText(saved.country, saved.values[PLACE], lang))
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.profile_title))
        for (scale in scales) {
            ScalePicker(scale, values[scale.key] ?: 0, { values[scale.key] = it }, context)
        }
        PlacePicker(place)
        BirthDateFields(birth)
        Button(onClick = { status = context.getString(saveDraft(file, values, birth, place)); saves++ },
            modifier = Modifier.testTag("profile-save")) {
            Text(stringResource(R.string.profile_save))
        }
        Text(status, Modifier.testTag("profile-status"))
        PreviewSection(file, scales, saves)
        OutlinedButton(onClick = onBack) { Text(stringResource(R.string.profile_back)) }
    }
}

@Composable
private fun BirthDateFields(birth: BirthInputs) {
    Text(stringResource(R.string.profile_birth_optional))
    OutlinedTextField(birth.year, { birth.year = it }, label = { Text(stringResource(R.string.profile_birth_year)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.testTag("profile-birth-year"))
    OutlinedTextField(birth.month, { birth.month = it }, label = { Text(stringResource(R.string.profile_birth_month)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.testTag("profile-birth-month"))
    OutlinedTextField(birth.day, { birth.day = it }, label = { Text(stringResource(R.string.profile_birth_day)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.testTag("profile-birth-day"))
}

@Composable
private fun ScalePicker(scale: ProfileScale, selected: Int, onSelect: (Int) -> Unit, context: Context) {
    var expanded by remember { mutableStateOf(false) }
    Text(label(context, "field.${scale.key}"))
    OutlinedButton(onClick = { expanded = true }, modifier = Modifier.testTag("profile-${scale.key}")) {
        val chosen = scale.choices.firstOrNull { it.code.toInt() == selected }
        val title = if (chosen == null) {
            stringResource(R.string.profile_unset)
        } else {
            label(context, "value.${scale.key}.${chosen.key}")
        }
        Text(title)
    }
    DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
        DropdownMenuItem(text = { Text(stringResource(R.string.profile_unset)) },
            onClick = { onSelect(0); expanded = false })
        for (choice in scale.choices) {
            DropdownMenuItem(text = { Text(label(context, "value.${scale.key}.${choice.key}")) },
                onClick = { onSelect(choice.code.toInt()); expanded = false })
        }
    }
}
