package dev.clawseed.demo.updater

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import dev.clawseed.demo.BuildConfig
import java.io.File

/**
 * Installs APK files using the system package installer.
 *
 * Handles:
 * - FileProvider URI generation (required on Android 7+)
 * - ACTION_INSTALL_PACKAGE intent
 * - REQUEST_INSTALL_PACKAGES permission check (Android 8+)
 */
object ApkInstaller {

    private const val FILE_PROVIDER_AUTHORITY = "${BuildConfig.APPLICATION_ID}.fileprovider"

    /**
     * Check if the app has permission to install packages from unknown sources.
     * On Android 8+ (API 26+), this requires the REQUEST_INSTALL_PACKAGES permission.
     */
    fun canInstallPackages(context: Context): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.packageManager.canRequestPackageInstalls()
        } else {
            // Pre-O: check global setting
            @Suppress("DEPRECATION")
            Settings.Secure.getInt(
                context.contentResolver,
                Settings.Secure.INSTALL_NON_MARKET_APPS,
                0
            ) == 1
        }
    }

    /**
     * Get an intent to open the system settings page for granting
     * installation permission from unknown sources.
     */
    fun installPermissionIntent(): Intent {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                data = Uri.parse("package:${BuildConfig.APPLICATION_ID}")
            }
        } else {
            @Suppress("DEPRECATION")
            Intent(Settings.ACTION_SECURITY_SETTINGS)
        }
    }

    /**
     * Install an APK file using the system package installer.
     *
     * @param context Activity context (required for startActivity).
     * @param apkFile The downloaded APK file.
     *
     * @throws IllegalStateException if the app lacks install permission.
     */
    fun install(context: Context, apkFile: File) {
        check(canInstallPackages(context)) {
            "App does not have permission to install packages from unknown sources"
        }

        val uri = FileProvider.getUriForFile(context, FILE_PROVIDER_AUTHORITY, apkFile)

        @Suppress("DEPRECATION")
        val intent = Intent(Intent.ACTION_INSTALL_PACKAGE).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            // Allow the installer to return a result
            putExtra(Intent.EXTRA_RETURN_RESULT, true)
        }

        intent.resolveActivity(context.packageManager)?.let {
            context.startActivity(intent)
        } ?: run {
            // Fallback: use ACTION_VIEW if ACTION_INSTALL_PACKAGE is not available
            val fallbackIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "application/vnd.android.package-archive")
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(fallbackIntent)
        }
    }
}
