package com.dyapp

import android.content.Context
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.assessBirthParts
import java.io.File
import java.time.LocalDate
import java.time.ZoneOffset

private fun possibleAgeOnUtcToday(saved: Map<String, Int>): String? {
    val today = LocalDate.now(ZoneOffset.UTC).toEpochDay().toULong()
    val ages = runCatching {
        assessBirthParts(saved[BIRTH_YEAR]?.toUInt(), saved[BIRTH_MONTH]?.toUInt(), saved[BIRTH_DAY]?.toUInt(), today)
    }.getOrNull()
    val (youngest, oldest) = ages?.youngest to ages?.oldest
    return when {
        youngest == null || oldest == null -> null
        youngest == oldest -> "$youngest"
        else -> "$youngest-$oldest"
    }
}

@Composable
internal fun PreviewSection(file: File, scales: List<ProfileScale>, saves: Int) {
    // Reads the saved file, so unsaved edits never appear; each save hides a stale preview.
    var preview by remember(saves) { mutableStateOf<Result<Map<String, Int>>?>(null) }
    OutlinedButton(onClick = { preview = runCatching { readDraft(file, scales) } },
        modifier = Modifier.testTag("profile-preview")) {
        Text(stringResource(R.string.profile_preview))
    }
    preview?.fold({ ProfilePreview(it, scales, LocalContext.current) },
        { Text(stringResource(R.string.profile_read_failed)) })
}

@Composable
private fun ProfilePreview(saved: Map<String, Int>, scales: List<ProfileScale>, context: Context) {
    val fields = scales.mapNotNull { scale ->
        scale.choices.firstOrNull { it.code.toInt() == saved[scale.key] }?.let { scale.key to it.key }
    }
    val birth = listOf(BIRTH_YEAR to R.string.profile_birth_year, BIRTH_MONTH to R.string.profile_birth_month,
        BIRTH_DAY to R.string.profile_birth_day).mapNotNull { (key, name) -> saved[key]?.let { name to it } }
    Text(stringResource(R.string.profile_preview_title))
    if (fields.isEmpty() && birth.isEmpty()) Text(stringResource(R.string.profile_preview_empty))
    for ((scale, choice) in fields) {
        Text(stringResource(R.string.profile_preview_row, label(context, "field.$scale"),
            label(context, "value.$scale.$choice")))
    }
    for ((name, value) in birth) Text(stringResource(R.string.profile_preview_row, stringResource(name), "$value"))
    val age = possibleAgeOnUtcToday(saved) ?: stringResource(R.string.profile_age_unknown)
    Text(stringResource(R.string.profile_age, age),
        Modifier.testTag("profile-preview-age"))
    Text(stringResource(R.string.profile_public_disclosure), Modifier.testTag("profile-public-disclosure"))
    if (BIRTH_KEYS.all { it in saved }) {
        Text(stringResource(R.string.profile_full_date_disclosure), Modifier.testTag("profile-full-date"))
    }
}
