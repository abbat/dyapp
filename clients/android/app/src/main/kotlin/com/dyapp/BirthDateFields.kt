package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp

private const val NO_PART = "-"
private const val LAST_MONTH = 12
private const val LAST_DAY = 31

// The year field takes more of the row than the two pickers.
private const val YEAR_WEIGHT = 1.2f

// The typed numbers of the basics card: the birth date parts and the height.
internal class BirthInputs(saved: Map<String, Int>) {
    var year by mutableStateOf(saved[BIRTH_YEAR]?.toString().orEmpty())
    var month by mutableStateOf(saved[BIRTH_MONTH]?.toString().orEmpty())
    var day by mutableStateOf(saved[BIRTH_DAY]?.toString().orEmpty())
    var height by mutableStateOf(saved[HEIGHT]?.toString().orEmpty())
}

@Composable
internal fun BirthDateFields(birth: BirthInputs) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        val year = stringResource(R.string.profile_birth_part_year)
        Column(Modifier.weight(YEAR_WEIGHT), Arrangement.spacedBy(4.dp)) {
            Caption(year)
            OutlinedTextField(birth.year, { birth.year = it }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.fillMaxWidth().testTag("birth-year").semantics { contentDescription = year })
        }
        BirthSelect(R.string.profile_birth_part_month, "birth-month", birth.month, LAST_MONTH) { birth.month = it }
        BirthSelect(R.string.profile_birth_part_day, "birth-day", birth.day, LAST_DAY) { birth.day = it }
    }
    parseBirthParts(birth.year, birth.month, birth.day)?.let(::possibleAgeOnUtcToday)?.let {
        Caption(stringResource(R.string.profile_age_search, it), Modifier.testTag("profile-age"))
    }
}

@Composable
@Suppress("LongParameterList") // Month and day differ in each of these.
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
