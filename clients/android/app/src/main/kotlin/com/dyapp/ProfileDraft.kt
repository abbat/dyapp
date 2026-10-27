package com.dyapp

import android.util.AtomicFile
import org.json.JSONObject
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.placeCountries
import uniffi.dyapp.placeName
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException

/** Scale, birth and place values by key, and the ISO country code or empty. */
internal data class ProfileDraft(val values: Map<String, Int> = emptyMap(), val country: String = "")

private fun validValue(key: String, value: Int, country: String, scales: List<ProfileScale>) = when (key) {
    in BIRTH_KEYS -> true
    PLACE -> value > 0 && placeName(country, value.toUInt(), "en") != null
    else -> scales.any { scale -> scale.key == key && (value == 0 || scale.choices.any { it.code.toInt() == value }) }
}

internal fun readDraft(file: File, scales: List<ProfileScale>): ProfileDraft {
    val bytes = try {
        AtomicFile(file).openRead().use { it.readBytes() }
    } catch (_: FileNotFoundException) {
        return ProfileDraft()
    }
    val json = JSONObject(bytes.toString(Charsets.UTF_8))
    val country = if (json.has(COUNTRY)) json.getString(COUNTRY) else ""
    require(country.isEmpty() || country in placeCountries())
    val values = json.keys().asSequence().filter { it != COUNTRY }.associateWith { key ->
        json.getInt(key).also { require(validValue(key, it, country, scales)) }
    }
    require(validBirthParts(values.filterKeys { it in BIRTH_KEYS }))
    return ProfileDraft(values, country)
}

internal fun writeDraft(file: File, draft: ProfileDraft) {
    val atomic = AtomicFile(file)
    val stream = atomic.startWrite()
    try {
        val json = JSONObject()
        draft.values.forEach { (key, value) -> json.put(key, value) }
        if (draft.country.isNotEmpty()) json.put(COUNTRY, draft.country)
        stream.write(json.toString().toByteArray(Charsets.UTF_8))
        atomic.finishWrite(stream)
    } catch (error: IOException) {
        atomic.failWrite(stream)
        throw error
    }
}
