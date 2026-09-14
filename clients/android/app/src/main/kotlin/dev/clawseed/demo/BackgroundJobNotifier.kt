package dev.clawseed.demo

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import dev.clawseed.sdk.core.model.ChatEvent
import java.util.concurrent.ConcurrentHashMap

object BackgroundJobNotifier {
    private val deliveredJobs = ConcurrentHashMap.newKeySet<String>()

    @Volatile
    private var appInForeground = false

    fun setAppInForeground(inForeground: Boolean) {
        appInForeground = inForeground
    }

    fun notifyIfBackground(
        context: Context,
        event: ChatEvent.BackgroundJobCompleted,
        sessionId: String?,
    ) {
        if (appInForeground || event.jobId.isBlank() || !deliveredJobs.add(event.jobId)) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) return

        val notificationManager =
            context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        notificationManager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.background_job_channel),
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = context.getString(R.string.background_job_channel_description)
            },
        )

        val intent = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            sessionId?.let { putExtra(MainActivity.EXTRA_SESSION_ID, it) }
        }
        val pendingIntent = PendingIntent.getActivity(
            context,
            event.jobId.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val succeeded = event.status == "succeeded"
        val title = if (succeeded) {
            context.getString(R.string.background_job_succeeded)
        } else {
            context.getString(R.string.background_job_finished, event.status)
        }
        val detail = when {
            event.error != null -> event.error
            event.exitCode != null -> context.getString(R.string.background_job_exit_code, event.exitCode)
            else -> context.getString(R.string.background_job_status, event.status)
        }
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(if (succeeded) android.R.drawable.ic_dialog_info else android.R.drawable.ic_dialog_alert)
            .setContentTitle(title)
            .setContentText(detail)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()
        notificationManager.notify(event.jobId.hashCode(), notification)
    }

    private const val CHANNEL_ID = "background_jobs"
}
