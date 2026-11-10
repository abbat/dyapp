package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import uniffi.dyapp.ProfileChoice
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.parseProfileSeeking

/** Who the owner is looking for; a saved [notLooking] comes without the other three. */
internal data class ProfileSeeking(
    val genders: List<UInt> = emptyList(),
    val ageFrom: UInt? = null,
    val ageTo: UInt? = null,
    val notLooking: Boolean = false,
)

/** The form's typed state; ranges and gender codes are checked by the shared core on save. */
internal class SeekingInputs(saved: ProfileSeeking) {
    var genders by mutableStateOf(saved.genders.toSet())
    val ageFrom = mutableStateOf(saved.ageFrom?.toString().orEmpty())
    val ageTo = mutableStateOf(saved.ageTo?.toString().orEmpty())
    var notLooking by mutableStateOf(saved.notLooking)

    /** Compared for unsaved edits; ignores fields disabled by the not-looking switch. */
    val tracked get() = if (notLooking) listOf(true) else listOf(genders, ageFrom.value, ageTo.value)

    /** Parse and normalize through the shared core; null means invalid input. */
    fun parsed(): ProfileSeeking? =
        parseProfileSeeking(genders.toList(), ageFrom.value, ageTo.value, notLooking)?.let {
            ProfileSeeking(it.genders, it.ageFrom, it.ageTo, it.notLooking)
        }

    /** Show the normalized value returned by the shared core after saving. */
    fun showSaved(saved: ProfileSeeking) {
        genders = saved.genders.toSet()
        ageFrom.value = saved.ageFrom?.toString().orEmpty()
        ageTo.value = saved.ageTo?.toString().orEmpty()
        notLooking = saved.notLooking
    }
}

private fun genderChoices(scales: List<ProfileScale>) = scales.firstOrNull { it.key == GENDER }?.choices.orEmpty()

@Composable
private fun GenderRow(choice: ProfileChoice, seeking: SeekingInputs) {
    val checked = choice.code in seeking.genders
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("profile-seeking-gender-${choice.code}")
            .toggleable(checked, enabled = !seeking.notLooking, role = Role.Checkbox) {
                seeking.genders = if (it) seeking.genders + choice.code else seeking.genders - choice.code
            },
        Arrangement.spacedBy(8.dp),
        Alignment.CenterVertically,
    ) {
        Checkbox(checked, onCheckedChange = null, enabled = !seeking.notLooking)
        Text(label("value.$GENDER.${choice.key}"))
    }
}

@Composable
private fun SeekingAge(age: MutableState<String>, name: Int, tag: String, enabled: Boolean) {
    OutlinedTextField(
        age.value,
        { age.value = it },
        Modifier.fillMaxWidth().testTag(tag),
        enabled = enabled,
        label = { Text(stringResource(name)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        singleLine = true
    )
}

/** The switch, then the choices it disables; they stay visible so that turning it off restores them before a save. */
@Composable
internal fun SeekingFields(scales: List<ProfileScale>, seeking: SeekingInputs) {
    CardTitle(R.string.profile_seeking_title)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("profile-not-looking")
            .toggleable(seeking.notLooking, role = Role.Switch) { seeking.notLooking = it },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(R.string.profile_not_looking), Modifier.weight(1f))
        Switch(seeking.notLooking, onCheckedChange = null)
    }
    Caption(stringResource(R.string.profile_not_looking_hint))
    Caption(stringResource(R.string.profile_seeking_genders))
    for (choice in genderChoices(scales)) GenderRow(choice, seeking)
    val open = !seeking.notLooking
    SeekingAge(seeking.ageFrom, R.string.profile_seeking_age_from, "profile-seeking-age-from", open)
    SeekingAge(seeking.ageTo, R.string.profile_seeking_age_to, "profile-seeking-age-to", open)
    Caption(stringResource(R.string.profile_seeking_hint))
}

@Composable
private fun SeekingRow(name: Int, value: String, tag: String) {
    Text(stringResource(R.string.profile_preview_row, stringResource(name), value), Modifier.testTag(tag))
}

/** The public rows of [seeking]; a gender code this client does not know is left out. */
@Composable
internal fun SeekingRows(seeking: ProfileSeeking, scales: List<ProfileScale>) {
    val choices = genderChoices(scales)
    val genders = seeking.genders.mapNotNull { code -> choices.firstOrNull { it.code == code } }
        .map { label("value.$GENDER.${it.key}") }
    if (seeking.notLooking) {
        SeekingRow(R.string.profile_not_looking, stringResource(R.string.profile_not_looking_on), "preview-not-looking")
    }
    if (genders.isNotEmpty()) {
        SeekingRow(R.string.profile_seeking_genders, genders.joinToString(", "), "preview-seeking-genders")
    }
    seeking.ageFrom?.let { SeekingRow(R.string.profile_seeking_age_from, "$it", "preview-seeking-age-from") }
    seeking.ageTo?.let { SeekingRow(R.string.profile_seeking_age_to, "$it", "preview-seeking-age-to") }
}
