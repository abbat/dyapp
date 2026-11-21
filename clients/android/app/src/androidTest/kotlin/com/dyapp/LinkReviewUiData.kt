package com.dyapp

import uniffi.dyapp.LinkReviewField
import uniffi.dyapp.LinkReviewPhoto
import uniffi.dyapp.LinkReviewSnapshot

/** The offered list is supplied directly: presentation must not recalculate differences. */
internal fun completeLinkReviewFixture(): LinkReviewSnapshot {
    val base = linkReviewFixture()
    val fields = base.local.fields.copy(
        gender = UInt.MAX_VALUE,
        name = "A complete public name",
        country = "DE",
        place = UInt.MAX_VALUE.toString(),
        birthYear = 2001u, birthMonth = null, birthDay = 31u,
        about = "A complete paragraph with long words and line breaks.\n".repeat(7),
        seekingGenders = listOf(UInt.MAX_VALUE, 60u),
        seekingAgeFrom = 18u, seekingAgeTo = 120u, notLooking = true
    )
    val photo = LinkReviewPhoto(ByteArray(32) { 1 }, listOf(ByteArray(32) { 2 }, ByteArray(32) { 3 }))
    val local = base.local.copy(
        fields = fields,
        incomeFrom = ULong.MAX_VALUE - 1uL, incomeTo = ULong.MAX_VALUE,
        hasKids = false, wantsMoreKids = null,
        interests = listOf("music", "tea, coffee; learning languages", "music"),
        photos = listOf(photo), educationLevel = UInt.MAX_VALUE,
        profession = "A complete profession", company = "Long company name ".repeat(8),
        industry = "Industry", careerStage = "Career stage", workLifeBalance = "Work and life"
    )
    val remote = local.copy(
        fields = fields.copy(name = "Verified remote name", country = "FR", place = "2988507"),
        photos = listOf(photo.copy(thumbnail = ByteArray(32) { 4 })),
        educationLevel = 999u
    )
    return base.copy(local = local, remote = remote, differences = LinkReviewField.entries.toList())
}
