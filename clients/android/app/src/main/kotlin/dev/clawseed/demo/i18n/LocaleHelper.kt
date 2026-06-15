package dev.clawseed.demo.i18n

import android.content.Context
import android.content.SharedPreferences
import java.util.Locale

/**
 * Helper for in-app language switching.
 *
 * Uses SharedPreferences for synchronous reads (required in attachBaseContext
 * before any async DataStore reads can complete). The language preference is
 * also mirrored to DataStore for the settings UI to observe.
 */
object LocaleHelper {

    private const val PREFS_NAME = "clawseed_locale"
    private const val KEY_LANGUAGE_MODE = "language_mode"

    fun getOverrideLocale(context: Context): Locale? {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val langCode = prefs.getString(KEY_LANGUAGE_MODE, null)
        return when (langCode) {
            "en" -> Locale("en")
            "zh" -> Locale("zh", "CN")
            "system", null -> null // follow system default
            else -> Locale(langCode)
        }
    }

    fun setLocale(context: Context, languageCode: String?) {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        prefs.edit().putString(KEY_LANGUAGE_MODE, languageCode).apply()
    }

    fun wrapContext(context: Context): Context {
        val locale = getOverrideLocale(context)
        if (locale == null) return context // system default
        val config = android.content.res.Configuration(context.resources.configuration)
        config.setLocale(locale)
        return context.createConfigurationContext(config)
    }
}
