package com.dyapp

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
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
import uniffi.dyapp.SearchCondition
import uniffi.dyapp.SearchOp
import uniffi.dyapp.searchAgeCondition
import uniffi.dyapp.searchHeightCondition

// Node field numbers, as the desktop page uses them.
private const val COUNTRY_FIELD = 4u
private const val HEIGHT_FIELD = 46u
private const val PLACE_FIELD = 20u
private const val BIRTH_AGE_FIELD = 1001u
private const val YOUNGEST = 18u
private const val OLDEST = 150u

private fun age(typed: String, blank: UInt) =
    if (typed.isBlank()) blank else typed.trim().toUIntOrNull() ?: throw IllegalArgumentException("invalid search age")

private fun height(typed: String) = typed.trim().takeIf { it.isNotEmpty() }?.let {
    it.toUIntOrNull() ?: throw IllegalArgumentException("invalid search height")
}

private fun text(field: UInt, value: String) = SearchCondition(field, SearchOp.EQ, emptyList(), listOf(value))

/** Filters for the fields every reached node offers; hidden ones never reach the query. */
internal class SearchFilters(private val scales: List<ProfileScale>) {
    var offered by mutableStateOf(emptyList<UInt>())
    var ageMin by mutableStateOf("")
    var ageMax by mutableStateOf("")
    var heightMin by mutableStateOf("")
    var heightMax by mutableStateOf("")
    val place = PlaceInputs("", null, "")
    val picked = mutableStateMapOf<String, Int>()
    val employer = EmployerFilter()

    val shownScales get() = scales.filter { it.field in offered }

    /**
     * Node conditions; an age that is no whole number or fits no birth date fails as `invalid search age`,
     * a height that is no whole number or an impossible range as `invalid search height`.
     */
    fun conditions(): List<SearchCondition> {
        val out = shownScales.mapNotNull { scale ->
            picked[scale.key]?.takeIf { it != 0 }?.let {
                SearchCondition(scale.field, SearchOp.EQ, listOf(it.toLong()), emptyList())
            }
        }.toMutableList()
        out += ranges()
        if (COUNTRY_FIELD in offered && place.country.isNotEmpty()) {
            out += text(COUNTRY_FIELD, place.country)
            // Typed text that is no picked suggestion leaves the place open.
            place.place?.takeIf { PLACE_FIELD in offered }?.let { out += text(PLACE_FIELD, it.toString()) }
        }
        return out
    }

    private fun ranges(): List<SearchCondition> {
        val ages = if (BIRTH_AGE_FIELD in offered && (ageMin.isNotBlank() || ageMax.isNotBlank())) {
            searchAgeCondition(age(ageMin, YOUNGEST), age(ageMax, OLDEST))
        } else {
            null
        }
        val heights = if (HEIGHT_FIELD in offered) searchHeightCondition(height(heightMin), height(heightMax)) else null
        return listOfNotNull(ages, heights)
    }
}

/**
 * Age, height, country and place, and scale filters, shown after a sample for the fields the nodes offered,
 * then the employer filter.
 */
@Composable
internal fun SearchFilterFields(filters: SearchFilters) {
    if (BIRTH_AGE_FIELD in filters.offered) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            AgeField(filters.ageMin, R.string.search_age_min, Modifier.weight(1f).testTag("search-age-min")) {
                filters.ageMin = it
            }
            AgeField(filters.ageMax, R.string.search_age_max, Modifier.weight(1f).testTag("search-age-max")) {
                filters.ageMax = it
            }
        }
    }
    if (HEIGHT_FIELD in filters.offered) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            AgeField(filters.heightMin, R.string.search_height_min, Modifier.weight(1f).testTag("search-height-min")) {
                filters.heightMin = it
            }
            AgeField(filters.heightMax, R.string.search_height_max, Modifier.weight(1f).testTag("search-height-max")) {
                filters.heightMax = it
            }
        }
    }
    if (COUNTRY_FIELD in filters.offered) {
        PlacePicker(filters.place, PLACE_FIELD in filters.offered, R.string.search_any, "search")
    }
    if (filters.shownScales.isNotEmpty()) MoreFilters(filters)
    EmployerFilterFields(filters.employer)
}

// The scales stay behind an expander, as on the profile form; a pick still applies while collapsed.
@Composable
private fun MoreFilters(filters: SearchFilters) {
    var open by rememberSaveable { mutableStateOf(false) }
    TextButton(
        onClick = { open = !open },
        Modifier.fillMaxWidth().testTag("search-more").semantics {
            if (open) collapse { open = false; true } else expand { open = true; true }
        }
    ) {
        Text(stringResource(R.string.search_more), Modifier.weight(1f))
        Icon(painterResource(R.drawable.ic_expand_more), null, Modifier.rotate(if (open) FLIPPED_DEGREES else 0f))
    }
    if (open) {
        for (scale in filters.shownScales) {
            ScalePicker(scale, filters.picked[scale.key] ?: 0, R.string.search_any) { filters.picked[scale.key] = it }
        }
    }
}

@Composable
private fun AgeField(value: String, label: Int, modifier: Modifier, onChange: (String) -> Unit) {
    OutlinedTextField(value, onChange, modifier, label = { Text(stringResource(label)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), singleLine = true)
}
