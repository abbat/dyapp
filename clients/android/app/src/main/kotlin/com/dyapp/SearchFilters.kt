package com.dyapp

import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.SearchCondition
import uniffi.dyapp.SearchOp
import uniffi.dyapp.searchAgeCondition

// Node field numbers, as the desktop page uses them.
private const val COUNTRY_FIELD = 4u
private const val PLACE_FIELD = 20u
private const val BIRTH_AGE_FIELD = 1001u
private const val YOUNGEST = 18u
private const val OLDEST = 150u

private fun age(typed: String, blank: UInt) =
    if (typed.isBlank()) blank else typed.trim().toUIntOrNull() ?: throw IllegalArgumentException("invalid search age")

private fun text(field: UInt, value: String) = SearchCondition(field, SearchOp.EQ, emptyList(), listOf(value))

/** Filters for the fields every reached node offers; hidden ones never reach the query. */
internal class SearchFilters(private val scales: List<ProfileScale>) {
    var offered by mutableStateOf(emptyList<UInt>())
    var ageMin by mutableStateOf("")
    var ageMax by mutableStateOf("")
    val place = PlaceInputs("", null, "")
    val picked = mutableStateMapOf<String, Int>()

    val shownScales get() = scales.filter { it.field in offered }

    /** Node conditions; an age that is no whole number or fits no birth date fails as `invalid search age`. */
    fun conditions(): List<SearchCondition> {
        val out = shownScales.mapNotNull { scale ->
            picked[scale.key]?.takeIf { it != 0 }?.let {
                SearchCondition(scale.field, SearchOp.EQ, listOf(it.toLong()), emptyList())
            }
        }.toMutableList()
        if (BIRTH_AGE_FIELD in offered && (ageMin.isNotBlank() || ageMax.isNotBlank())) {
            out += searchAgeCondition(age(ageMin, YOUNGEST), age(ageMax, OLDEST))
        }
        if (COUNTRY_FIELD in offered && place.country.isNotEmpty()) {
            out += text(COUNTRY_FIELD, place.country)
            // Typed text that is no picked suggestion leaves the place open.
            place.place?.takeIf { PLACE_FIELD in offered }?.let { out += text(PLACE_FIELD, it.toString()) }
        }
        return out
    }
}

/** Age, country and place, and scale filters, shown after a sample for the fields the nodes offered. */
@Composable
internal fun SearchFilterFields(filters: SearchFilters) {
    if (BIRTH_AGE_FIELD in filters.offered) {
        AgeField(filters.ageMin, R.string.search_age_min, "search-age-min") { filters.ageMin = it }
        AgeField(filters.ageMax, R.string.search_age_max, "search-age-max") { filters.ageMax = it }
    }
    if (COUNTRY_FIELD in filters.offered) {
        PlacePicker(filters.place, PLACE_FIELD in filters.offered, R.string.search_any)
    }
    for (scale in filters.shownScales) {
        ScalePicker(scale, filters.picked[scale.key] ?: 0, R.string.search_any) { filters.picked[scale.key] = it }
    }
}

@Composable
private fun AgeField(value: String, label: Int, tag: String, onChange: (String) -> Unit) {
    OutlinedTextField(value, onChange, label = { Text(stringResource(label)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), singleLine = true,
        modifier = Modifier.testTag(tag))
}
