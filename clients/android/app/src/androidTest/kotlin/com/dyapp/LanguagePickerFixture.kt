package com.dyapp

/** Synthetic choices test arbitrary count/length; they do not advertise packaged translations. */
internal val languageFixture = listOf(
    LanguageOption(null, "System fixture"),
    LanguageOption("en", "English fixture"),
    LanguageOption("ru", "Russian fixture"),
    LanguageOption("de", "German fixture"),
    LanguageOption("fr", "A complete language name with regional and script details ".repeat(3)),
    LanguageOption("zh-Hans", "Chinese fixture"),
)
