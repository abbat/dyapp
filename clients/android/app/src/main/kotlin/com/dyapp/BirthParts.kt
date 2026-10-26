package com.dyapp

import java.time.DateTimeException
import java.time.LocalDate

private const val MAX_BIRTH_YEAR = 9999
private const val MONTHS_PER_YEAR = 12
private const val MAX_BIRTH_DAY = 31
private const val LEAP_REFERENCE_YEAR = 2000
internal const val BIRTH_YEAR = "birth_year"
internal const val BIRTH_MONTH = "birth_month"
internal const val BIRTH_DAY = "birth_day"
internal val BIRTH_KEYS = setOf(BIRTH_YEAR, BIRTH_MONTH, BIRTH_DAY)

/** Parse independently optional birth parts; null means invalid input, not an absent date. */
internal fun parseBirthParts(year: String, month: String, day: String): Map<String, Int>? {
    val input = mapOf(BIRTH_YEAR to year, BIRTH_MONTH to month, BIRTH_DAY to day)
        .filterValues { it.isNotBlank() }
    val parts = input.mapValues { (_, value) -> value.trim().toIntOrNull() ?: return null }
    return parts.takeIf(::validBirthParts)
}

private fun isPossibleDate(year: Int, month: Int, day: Int): Boolean = try {
    LocalDate.of(year, month, day)
    true
} catch (_: DateTimeException) {
    false
}

private fun hasPossibleMonth(year: Int, month: Int?, day: Int?): Boolean =
    (1..MONTHS_PER_YEAR).any { candidate ->
        (month == null || month == candidate) && isPossibleDate(year, candidate, day ?: 1)
    }

/** Mirrors the Rust profile core's Gregorian partial-date rule before saving a local draft. */
internal fun validBirthParts(values: Map<String, Int>): Boolean {
    val year = values[BIRTH_YEAR] ?: LEAP_REFERENCE_YEAR
    val month = values[BIRTH_MONTH]
    val day = values[BIRTH_DAY]
    val yearValid = year in 1..MAX_BIRTH_YEAR
    val monthValid = month == null || month in 1..MONTHS_PER_YEAR
    val dayValid = day == null || day in 1..MAX_BIRTH_DAY
    val keysValid = values.keys.all { it in BIRTH_KEYS }
    val dateValid = hasPossibleMonth(year, month, day)
    return yearValid && monthValid && dayValid && keysValid && dateValid
}
