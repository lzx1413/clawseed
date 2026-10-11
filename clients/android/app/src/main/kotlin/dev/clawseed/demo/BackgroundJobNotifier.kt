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
    private val deliveredChatEvents = ConcurrentHashMap.newKeySet<String>()

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

    fun notifyChatCompleted(context: Context, sessionId: String?, response: String) {
        notifyChatResult(context, sessionId, succeeded = true, detail = response)
    }

    fun notifyChatFailed(context: Context, sessionId: String?, error: String) {
        notifyChatResult(context, sessionId, succeeded = false, detail = error)
    }

    private fun notifyChatResult(
        context: Context,
        sessionId: String?,
        succeeded: Boolean,
        detail: String,
    ) {
        if (appInForeground || sessionId.isNullOrBlank()) return
        val cleanDetail = detail.replace(Regex("\\s+"), " ").trim()
        val eventKey = "$sessionId:$succeeded:$cleanDetail"
        if (!deliveredChatEvents.add(eventKey)) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) return

        val notificationManager =
            context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        notificationManager.createNotificationChannel(
            NotificationChannel(
                CHAT_CHANNEL_ID,
                context.getString(R.string.chat_background_channel),
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = context.getString(R.string.chat_background_channel_description)
            },
        )

        val intent = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(MainActivity.EXTRA_SESSION_ID, sessionId)
        }
        val pendingIntent = PendingIntent.getActivity(
            context,
            sessionId.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val title = context.getString(
            if (succeeded) R.string.chat_background_completed else R.string.chat_background_failed,
        )
        val visibleDetail = cleanDetail.ifBlank {
            context.getString(R.string.chat_background_response_empty)
        }.take(180)
        val notification = NotificationCompat.Builder(context, CHAT_CHANNEL_ID)
            .setSmallIcon(if (succeeded) android.R.drawable.ic_dialog_info else android.R.drawable.ic_dialog_alert)
            .setContentTitle(title)
            .setContentText(visibleDetail)
            .setStyle(NotificationCompat.BigTextStyle().bigText(visibleDetail))
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()
        notificationManager.notify(sessionId.hashCode(), notification)
    }

    private const val CHANNEL_ID = "background_jobs"
    private const val CHAT_CHANNEL_ID = "chat_completions"
}
