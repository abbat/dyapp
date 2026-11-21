package com.dyapp

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewProfile
import uniffi.dyapp.ProfileScale
import uniffi.dyapp.PublicProfileDraft
import uniffi.dyapp.placeName
import java.util.Locale

private fun reviewTexts(profile: LinkReviewProfile) = mapOf(
    LinkReviewField.NAME to profile.fields.name,
    LinkReviewField.INTERESTS to profile.interests.joinToString("; "),
    LinkReviewField.PROFESSION to profile.profession,
    LinkReviewField.COMPANY to profile.company,
    LinkReviewField.INDUSTRY to profile.industry,
    LinkReviewField.CAREER_STAGE to profile.careerStage,
    LinkReviewField.WORK_LIFE_BALANCE to profile.workLifeBalance,
    LinkReviewField.ABOUT to profile.fields.about,
    LinkReviewField.HEIGHT to profile.fields.height?.toString().orEmpty()
)

private fun reviewScalars(profile: LinkReviewProfile): Map<LinkReviewField, UInt> {
    val fields = profile.fields
    return mapOf(
        LinkReviewField.EDUCATION to profile.educationLevel,
        LinkReviewField.GENDER to fields.gender,
        LinkReviewField.ORIENTATION to fields.orientation,
        LinkReviewField.RELATIONSHIP_GOAL to fields.relationshipGoal,
        LinkReviewField.ROLE_PREFERENCE to fields.rolePreference,
        LinkReviewField.CAREER_AMBITION to fields.careerAmbition,
        LinkReviewField.FINANCIAL_PHILOSOPHY to fields.financialPhilosophy,
        LinkReviewField.EMPLOYMENT_STATUS to fields.employmentStatus,
        LinkReviewField.FITNESS_LEVEL to fields.fitnessLevel,
        LinkReviewField.ZODIAC to fields.zodiac,
        LinkReviewField.SMOKING to fields.smoking,
        LinkReviewField.DRINKING to fields.drinking,
        LinkReviewField.TATTOOS to fields.tattoos,
        LinkReviewField.MARITAL_STATUS to fields.maritalStatus,
        LinkReviewField.PETS to fields.pets
    )
}

/** Full frozen values: no Int narrowing, truncation, age inference or use of the editable draft adapter. */
@Composable
internal fun linkReviewValue(field: LinkReviewField, profile: LinkReviewProfile, scales: List<ProfileScale>): String {
    val text = reviewTexts(profile)[field]
    val scalar = reviewScalars(profile)[field]
    val key = if (field == LinkReviewField.EDUCATION) "education_level" else field.name.lowercase(Locale.ROOT)
    return when {
        text != null -> reviewText(text)
        scalar != null -> reviewScale(key, scalar, scales)
        else -> reviewGroup(field, profile, scales)
    }
}

@Composable
private fun reviewGroup(field: LinkReviewField, profile: LinkReviewProfile, scales: List<ProfileScale>): String =
    when (field) {
        LinkReviewField.LOCATION -> reviewLocation(profile.fields)
        LinkReviewField.INCOME -> listOf(
            reviewRow(R.string.link_review_income_from, reviewText(profile.incomeFrom?.toString().orEmpty())),
            reviewRow(R.string.link_review_income_to, reviewText(profile.incomeTo?.toString().orEmpty()))
        ).joinToString("\n")
        LinkReviewField.HAS_KIDS -> reviewBoolean(profile.hasKids)
        LinkReviewField.WANTS_MORE_KIDS -> reviewBoolean(profile.wantsMoreKids)
        LinkReviewField.BIRTH -> reviewBirth(profile.fields)
        LinkReviewField.SEEKING -> reviewSeeking(profile.fields, scales)
        LinkReviewField.PHOTOS -> stringResource(R.string.link_review_photo_count, profile.photos.size)
        else -> error("Missing frozen comparison field: $field")
    }

@Composable
private fun reviewBirth(fields: PublicProfileDraft) = listOf(
    reviewRow(R.string.profile_birth_year, reviewNumber(fields.birthYear)),
    reviewRow(R.string.profile_birth_month, reviewNumber(fields.birthMonth)),
    reviewRow(R.string.profile_birth_day, reviewNumber(fields.birthDay))
).joinToString("\n")

@Composable
private fun reviewLocation(fields: PublicProfileDraft): String {
    val locale = LocalConfiguration.current.locales[0]
    val country = runCatching { countryName(fields.country, locale) }.getOrNull()
    val place = fields.place.toUIntOrNull()?.let { placeName(fields.country, it, placeLang()) }?.let(::placeLabel)
    return listOf(
        reviewRow(R.string.profile_country, reviewNamedCode(country, fields.country)),
        reviewRow(R.string.profile_place, reviewNamedCode(place, fields.place))
    ).joinToString("\n")
}

@Composable
private fun reviewSeeking(fields: PublicProfileDraft, scales: List<ProfileScale>) = listOf(
    reviewRow(R.string.profile_not_looking, reviewBoolean(fields.notLooking)),
    reviewRow(
        R.string.profile_seeking_genders,
        reviewText(fields.seekingGenders.map { reviewScale("gender", it, scales) }.joinToString("; "))
    ),
    reviewRow(R.string.profile_seeking_age_from, reviewNumber(fields.seekingAgeFrom)),
    reviewRow(R.string.profile_seeking_age_to, reviewNumber(fields.seekingAgeTo))
).joinToString("\n")
