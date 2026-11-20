package com.dyapp

import android.content.Context
import org.xmlpull.v1.XmlPullParser
import java.util.Locale

private const val ANDROID_XML_NAMESPACE = "http://schemas.android.com/apk/res/android"

/** Application-owned resource inventory, independent of locales supplied by dependency libraries. */
internal fun Context.packagedLanguages(): List<String> {
    val tags = mutableListOf<String>()
    resources.getXml(R.xml.app_languages).use { parser ->
        while (parser.eventType != XmlPullParser.END_DOCUMENT) {
            if (parser.eventType == XmlPullParser.START_TAG && parser.name == "locale") {
                tags += checkNotNull(parser.getAttributeValue(ANDROID_XML_NAMESPACE, "name"))
            }
            parser.next()
        }
    }
    return tags
}

/** Old enum names match their tags; absent, system or removed/unknown choices follow the system. */
internal fun selectedLanguage(saved: String?, available: List<String>): String? =
    available.firstOrNull { it.equals(saved, ignoreCase = true) }

internal fun languageName(tag: String): String {
    val locale = Locale.forLanguageTag(tag)
    return locale.getDisplayName(locale).ifBlank { tag }
}
