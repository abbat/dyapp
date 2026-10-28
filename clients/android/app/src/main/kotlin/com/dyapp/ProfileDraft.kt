package com.dyapp

import android.util.AtomicFile
import org.json.JSONObject
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.PublicProfileDraft
import uniffi.dyapp.loadProfileDraft
import uniffi.dyapp.placeCountries
import uniffi.dyapp.placeName
import uniffi.dyapp.saveProfileDraft
import java.io.File
import java.io.FileNotFoundException

private const val LEGACY_PROFILE_DRAFT_JSON = "profile-draft.json"

// Rust scale keys of the record fields.
private const val GENDER = "gender"
private const val ORIENTATION = "orientation"
private const val RELATIONSHIP_GOAL = "relationship_goal"
private const val ROLE_PREFERENCE = "role_preference"
private const val CAREER_AMBITION = "career_ambition"
private const val FINANCIAL_PHILOSOPHY = "financial_philosophy"
private const val EMPLOYMENT_STATUS = "employment_status"
private const val FITNESS_LEVEL = "fitness_level"
private const val ZODIAC = "zodiac"

/** Scale, birth and place values by key, and the ISO country code or empty. */
internal data class ProfileDraft(val values: Map<String, Int> = emptyMap(), val country: String = "")

private fun validValue(key: String, value: Int, country: String, scales: List<ProfileScale>) = when (key) {
    in BIRTH_KEYS -> true
    PLACE -> value > 0 && placeName(country, value.toUInt(), "en") != null
    else -> scales.any { scale -> scale.key == key && (value == 0 || scale.choices.any { it.code.toInt() == value }) }
}

private fun readDraft(file: File, scales: List<ProfileScale>): ProfileDraft {
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

/** The Rust record of [this]; absent keys stay unpublished. Keys are the Rust scale keys. */
internal fun ProfileDraft.record(): PublicProfileDraft {
    fun code(key: String) = (values[key] ?: 0).toUInt()
    return PublicProfileDraft(
        gender = code(GENDER), orientation = code(ORIENTATION),
        relationshipGoal = code(RELATIONSHIP_GOAL), rolePreference = code(ROLE_PREFERENCE),
        careerAmbition = code(CAREER_AMBITION), financialPhilosophy = code(FINANCIAL_PHILOSOPHY),
        employmentStatus = code(EMPLOYMENT_STATUS), fitnessLevel = code(FITNESS_LEVEL),
        zodiac = code(ZODIAC), birthYear = values[BIRTH_YEAR]?.toUInt(),
        birthMonth = values[BIRTH_MONTH]?.toUInt(), birthDay = values[BIRTH_DAY]?.toUInt(),
        country = country, place = values[PLACE]?.toString().orEmpty(),
    )
}

/** The editor values of a Rust record; zero scales and absent parts are left out. */
internal fun PublicProfileDraft.draft(): ProfileDraft {
    val scales = listOf(GENDER to gender, ORIENTATION to orientation,
        RELATIONSHIP_GOAL to relationshipGoal, ROLE_PREFERENCE to rolePreference,
        CAREER_AMBITION to careerAmbition, FINANCIAL_PHILOSOPHY to financialPhilosophy,
        EMPLOYMENT_STATUS to employmentStatus, FITNESS_LEVEL to fitnessLevel, ZODIAC to zodiac)
    val optional = listOf(BIRTH_YEAR to birthYear, BIRTH_MONTH to birthMonth, BIRTH_DAY to birthDay,
        PLACE to place.toUIntOrNull())
    val values = scales.filter { it.second != 0u } + optional.mapNotNull { (key, value) -> value?.let { key to it } }
    return ProfileDraft(values.associate { (key, value) -> key to value.toInt() }, country)
}

/** This identity's `client.db` draft; an older `profile-draft.json` is imported once and kept. */
internal fun loadDraft(opened: Opened.Ready, scales: List<ProfileScale>): ProfileDraft =
    RustBridge.withKey(opened) { dir, key ->
        loadProfileDraft(dir, key)?.draft()
            ?: readDraft(File(opened.slot.dir, LEGACY_PROFILE_DRAFT_JSON), scales).also {
                if (it != ProfileDraft()) saveProfileDraft(dir, key, it.record())
            }
    }

/** Replace the editable fields of this identity's `client.db` draft, keeping its proof progress. */
internal fun saveDraft(opened: Opened.Ready, draft: ProfileDraft) =
    RustBridge.withKey(opened) { dir, key -> saveProfileDraft(dir, key, draft.record()) }
