package com.dyapp

import uniffi.dyapp.PublicProfileDraft
import uniffi.dyapp.loadProfileDraft
import uniffi.dyapp.parseProfileHeight
import uniffi.dyapp.saveProfileDraft

// Rust scale keys of the record fields.
internal const val GENDER = "gender"
private const val ORIENTATION = "orientation"
private const val RELATIONSHIP_GOAL = "relationship_goal"
private const val ROLE_PREFERENCE = "role_preference"
private const val CAREER_AMBITION = "career_ambition"
private const val FINANCIAL_PHILOSOPHY = "financial_philosophy"
private const val EMPLOYMENT_STATUS = "employment_status"
private const val FITNESS_LEVEL = "fitness_level"
private const val ZODIAC = "zodiac"
private const val SMOKING = "smoking"
private const val DRINKING = "drinking"
private const val TATTOOS = "tattoos"
private const val MARITAL_STATUS = "marital_status"
private const val PETS = "pets"
internal const val HEIGHT = "height"

/** Scale, birth, height and place values by key, the ISO country code or empty, and the optional public texts. */
internal data class ProfileDraft(
    val values: Map<String, Int> = emptyMap(),
    val country: String = "",
    val name: String = "",
    val about: String = "",
    val seeking: ProfileSeeking = ProfileSeeking(),
)

/** The shared core parses and checks height; an empty map clears it and null rejects it. */
internal fun parsedHeight(typed: String): Map<String, Int>? = parseProfileHeight(typed)

/** The Rust record of [this]; absent keys stay unpublished. Keys are the Rust scale keys. */
internal fun ProfileDraft.record(): PublicProfileDraft {
    fun code(key: String) = (values[key] ?: 0).toUInt()
    return PublicProfileDraft(
        name = name, gender = code(GENDER), orientation = code(ORIENTATION),
        relationshipGoal = code(RELATIONSHIP_GOAL), rolePreference = code(ROLE_PREFERENCE),
        careerAmbition = code(CAREER_AMBITION), financialPhilosophy = code(FINANCIAL_PHILOSOPHY),
        employmentStatus = code(EMPLOYMENT_STATUS), fitnessLevel = code(FITNESS_LEVEL),
        zodiac = code(ZODIAC), birthYear = values[BIRTH_YEAR]?.toUInt(),
        birthMonth = values[BIRTH_MONTH]?.toUInt(), birthDay = values[BIRTH_DAY]?.toUInt(),
        country = country, place = values[PLACE]?.toString().orEmpty(), height = values[HEIGHT]?.toUInt(),
        about = about, smoking = code(SMOKING), drinking = code(DRINKING), tattoos = code(TATTOOS),
        maritalStatus = code(MARITAL_STATUS), pets = code(PETS), seekingGenders = seeking.genders,
        seekingAgeFrom = seeking.ageFrom, seekingAgeTo = seeking.ageTo, notLooking = seeking.notLooking,
    )
}

/** The editor values of a Rust record; zero scales and absent parts are left out. */
internal fun PublicProfileDraft.draft(): ProfileDraft {
    val scales = listOf(
        GENDER to gender, ORIENTATION to orientation,
        RELATIONSHIP_GOAL to relationshipGoal, ROLE_PREFERENCE to rolePreference,
        CAREER_AMBITION to careerAmbition, FINANCIAL_PHILOSOPHY to financialPhilosophy,
        EMPLOYMENT_STATUS to employmentStatus, FITNESS_LEVEL to fitnessLevel, ZODIAC to zodiac,
        SMOKING to smoking, DRINKING to drinking, TATTOOS to tattoos, MARITAL_STATUS to maritalStatus, PETS to pets
    )
    val optional = listOf(
        BIRTH_YEAR to birthYear,
        BIRTH_MONTH to birthMonth,
        BIRTH_DAY to birthDay,
        PLACE to place.toUIntOrNull(),
        HEIGHT to height
    )
    val values = scales.filter { it.second != 0u } + optional.mapNotNull { (key, value) -> value?.let { key to it } }
    return ProfileDraft(
        values.associate { (key, value) -> key to value.toInt() },
        country,
        name,
        about,
        ProfileSeeking(seekingGenders, seekingAgeFrom, seekingAgeTo, notLooking)
    )
}

/** Only a missing core draft starts empty; key/database errors propagate without a fallback. */
internal fun loadDraft(opened: Opened.Ready): ProfileDraft =
    RustBridge.withKey(opened) { dir, key -> loadProfileDraft(dir, key)?.draft() ?: ProfileDraft() }

/** Replace the editable fields of this identity's `client.db` draft, keeping its proof progress and photos. */
internal fun saveDraft(opened: Opened.Ready, draft: ProfileDraft) = synchronized(RustBridge.draftWrites) {
    RustBridge.withKey(opened) { dir, key -> saveProfileDraft(dir, key, draft.record()) }
}
