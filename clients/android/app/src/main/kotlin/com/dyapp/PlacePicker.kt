package com.dyapp

import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.Place
import uniffi.dyapp.placeCountries
import uniffi.dyapp.placeName
import uniffi.dyapp.searchPlaces
import java.util.Locale

internal const val COUNTRY = "country"
internal const val PLACE = "place"

internal fun placeLabel(place: Place) = if (place.region.isEmpty()) place.name else "${place.name}, ${place.region}"

internal fun placeText(country: String, place: Int?, lang: String) =
    place?.let { placeName(country, it.toUInt(), lang) }?.let(::placeLabel).orEmpty()

internal fun countryName(code: String, locale: Locale): String =
    Locale.Builder().setRegion(code).build().getDisplayCountry(locale)

/** The UI language for Rust place names: `ru` gives Russian, any other English. */
@Composable
internal fun placeLang(): String = LocalConfiguration.current.locales[0].language

/** Country and place being edited; `place` is set only by picking a suggestion. */
internal class PlaceInputs(country: String, place: Int?, text: String) {
    var country by mutableStateOf(country)
    var place by mutableStateOf(place)
    var text by mutableStateOf(text)

    /** Typed text that is no picked suggestion. */
    val unknown get() = place == null && text.isNotEmpty()

    fun pickCountry(code: String) {
        if (code != country) {
            country = code
            place = null
            text = ""
        }
    }
}

@Composable
internal fun PlacePicker(inputs: PlaceInputs) {
    val locale = LocalConfiguration.current.locales[0]
    val countries = remember(locale) {
        placeCountries().map { it to countryName(it, locale) }.sortedBy { it.second }
    }
    var expanded by remember { mutableStateOf(false) }
    Text(stringResource(R.string.profile_country))
    OutlinedButton(onClick = { expanded = true }, modifier = Modifier.testTag("profile-country")) {
        Text(countries.firstOrNull { it.first == inputs.country }?.second ?: stringResource(R.string.profile_unset))
    }
    DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
        DropdownMenuItem(text = { Text(stringResource(R.string.profile_unset)) },
            onClick = { inputs.pickCountry(""); expanded = false }, modifier = Modifier.testTag("country-none"))
        for ((code, name) in countries) {
            DropdownMenuItem(text = { Text(name) }, onClick = { inputs.pickCountry(code); expanded = false },
                modifier = Modifier.testTag("country-$code"))
        }
    }
    OutlinedTextField(inputs.text, { inputs.text = it; inputs.place = null },
        label = { Text(stringResource(R.string.profile_place)) }, enabled = inputs.country.isNotEmpty(),
        singleLine = true, modifier = Modifier.testTag("profile-place"))
    if (inputs.unknown) PlaceSuggestions(inputs, placeLang())
}

@Composable
private fun PlaceSuggestions(inputs: PlaceInputs, lang: String) {
    val found = remember(inputs.country, inputs.text, lang) { searchPlaces(inputs.country, inputs.text, lang, 5u) }
    for (place in found) {
        TextButton(onClick = { inputs.place = place.id.toInt(); inputs.text = placeLabel(place) },
            modifier = Modifier.testTag("place-${place.id}")) { Text(placeLabel(place)) }
    }
}
