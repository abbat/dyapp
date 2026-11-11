package com.dyapp

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** One screen owner's typed inputs and the exact input snapshot last persisted. UI thread only. */
internal class ProfileEditor(saved: ProfileDraft, lang: String) {
    val values = mutableStateMapOf<String, Int>().apply { putAll(saved.values) }
    val birth = BirthInputs(saved.values, saved.about, saved.seeking)
    val name = mutableStateOf(saved.name)
    val place = PlaceInputs(saved.country, saved.values[PLACE], placeText(saved.country, saved.values[PLACE], lang))
    private val edits get() = inputs(values, birth, place) + name.value
    private var savedEdits by mutableStateOf(edits)
    val unsaved get() = edits != savedEdits

    suspend fun save(persist: (ProfileDraft) -> Unit): Int {
        val submitted = edits
        val result = saveEdits(ProfileDraft(values.toMap(), name = name.value), birth, place, persist)
        if (result.status == R.string.profile_saved) {
            if (edits == submitted) {
                birth.seeking.showSaved(checkNotNull(result.seeking))
                savedEdits = edits
            } else {
                // An older save must neither normalize newer inputs nor mark them saved.
                savedEdits = submitted
            }
        }
        return result.status
    }
}

private fun inputs(values: Map<String, Int>, birth: BirthInputs, place: PlaceInputs) =
    listOf(
        values.toMap(), birth.year, birth.month, birth.day, birth.height, birth.about, place.country, place.place,
        place.text, birth.seeking.tracked
    )
