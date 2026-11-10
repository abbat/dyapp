package com.dyapp

import uniffi.dyapp.parseProfileBirthParts
import uniffi.dyapp.validProfileBirthParts

internal const val BIRTH_YEAR = "birth_year"
internal const val BIRTH_MONTH = "birth_month"
internal const val BIRTH_DAY = "birth_day"
internal val BIRTH_KEYS = setOf(BIRTH_YEAR, BIRTH_MONTH, BIRTH_DAY)

/** Parse independently optional birth parts; null means invalid input, not an absent date. */
internal fun parseBirthParts(year: String, month: String, day: String): Map<String, Int>? =
    parseProfileBirthParts(year, month, day)

/** The same shared Gregorian rule that validates published profiles. */
internal fun validBirthParts(values: Map<String, Int>): Boolean = validProfileBirthParts(values)
