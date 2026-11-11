package com.dyapp

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private val REPLACED_KEYS = BIRTH_KEYS + PLACE + HEIGHT

internal data class ProfileSaveResult(val status: Int, val seeking: ProfileSeeking? = null)

// Stable core failures map only to presentation text here.
private fun saveFailure(error: Throwable) = when {
    "invalidName" in error.toString() -> R.string.profile_name_invalid
    "invalidHeight" in error.toString() -> R.string.profile_height_invalid
    "invalidAbout" in error.toString() -> R.string.profile_about_invalid
    "invalidSeeking" in error.toString() -> R.string.profile_seeking_invalid
    else -> R.string.profile_save_failed
}

/** Capture typed UI values before IO; parsing and storage validation remain in the core. */
internal suspend fun saveEdits(
    edited: ProfileDraft,
    birth: BirthInputs,
    place: PlaceInputs,
    persist: (ProfileDraft) -> Unit,
): ProfileSaveResult {
    val parts = parseBirthParts(birth.year, birth.month, birth.day)
    val tall = parsedHeight(birth.height)
    val seeking = birth.seeking.parsed()
    val picked = place.place?.let { mapOf(PLACE to it) }.orEmpty()
    val kept = edited.values.filterKeys { it !in REPLACED_KEYS }
    return when {
        parts == null -> ProfileSaveResult(R.string.profile_birth_invalid)
        tall == null -> ProfileSaveResult(R.string.profile_height_invalid)
        place.unknown -> ProfileSaveResult(R.string.profile_place_unknown)
        seeking == null -> ProfileSaveResult(R.string.profile_seeking_invalid)
        else -> persistEdits(
            edited.copy(
                values = kept + parts + picked + tall,
                country = place.country,
                about = birth.about,
                seeking = seeking
            ),
            persist
        )
    }
}

private suspend fun persistEdits(draft: ProfileDraft, persist: (ProfileDraft) -> Unit): ProfileSaveResult =
    withContext(Dispatchers.IO) { runCatching { persist(draft) } }
        .fold({ ProfileSaveResult(R.string.profile_saved, draft.seeking) }, { ProfileSaveResult(saveFailure(it)) })
