package dev.clawseed.demo.ui.components

import android.content.res.Configuration
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import androidx.core.view.WindowCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.demo.MainActivity
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.i18n.LocaleHelper
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Test

class FollowSystemThemeDeviceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    private fun nightMode(arg: String = ""): String {
        val fd = instrumentation.uiAutomation.executeShellCommand("cmd uimode night $arg")
        return ParcelFileDescriptor.AutoCloseInputStream(fd).bufferedReader().use { it.readText().trim() }
    }

    @Test fun followsNightModeWhileUsingAnExplicitAppLanguage() = runBlocking {
        val originalNight = nightMode().substringAfter("Night mode: ")
        assumeTrue(originalNight in setOf("yes", "no"))
        val store = LocalStore(instrumentation.targetContext.applicationContext)
        val originalTheme = store.themeMode.first()
        val originalLanguage = store.languageMode.first()
        try {
            nightMode("no")
            store.setLanguageMode("en")
            store.setThemeMode("dark")
            ActivityScenario.launch(MainActivity::class.java).use { activity ->
                awaitTheme(activity, true)
                store.setThemeMode("system")
                awaitTheme(activity, false)
                val localized = LocaleHelper.wrapContext(instrumentation.targetContext.applicationContext)
                nightMode("yes")
                awaitTheme(activity, true)
                assertEquals(Configuration.UI_MODE_NIGHT_YES, localized.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK)
                store.setThemeMode("light")
                awaitTheme(activity, false)
                store.setThemeMode("system")
                awaitTheme(activity, true)
                nightMode("no")
                awaitTheme(activity, false)
                assertEquals(Configuration.UI_MODE_NIGHT_NO, localized.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK)
            }
            nightMode("yes")
            ActivityScenario.launch(MainActivity::class.java).use { activity ->
                assertEquals("system", store.themeMode.first())
                awaitTheme(activity, true)
            }
        } finally {
            nightMode(originalNight)
            store.setLanguageMode(originalLanguage)
            store.setThemeMode(originalTheme)
        }
    }

    private fun awaitTheme(scenario: ActivityScenario<MainActivity>, dark: Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 8000
        var details = ""
        do {
            var matches = false
            scenario.onActivity { activity ->
                val window = activity.window
                val lightIconsBackground = WindowCompat.getInsetsController(window, window.decorView)
                    .isAppearanceLightStatusBars
                val config = activity.resources.configuration
                details = "night bits=${config.uiMode and Configuration.UI_MODE_NIGHT_MASK}, light status bar=$lightIconsBackground"
                matches = lightIconsBackground == !dark && config.locales[0].language == "en"
            }
            if (matches) return
            SystemClock.sleep(100)
        } while (SystemClock.elapsedRealtime() < deadline)
        fail("Expected dark=$dark with English UI, got $details")
    }
}
