package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import android.net.Uri
import java.io.File
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.sdk.core.model.MediaReference
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@Composable
internal fun AudioContentCard(
    media: MediaReference,
    title: String?,
    readAsset: (suspend (String) -> Result<ByteArray>)?,
    modifier: Modifier = Modifier,
) {
    val url = media.safeHttpsUrl()
    val context = androidx.compose.ui.platform.LocalContext.current
    val assetId = media.assetId?.takeIf { it.isNotBlank() }
    val localPath = media.path?.takeIf { it.isNotBlank() }
    var localFile by remember(localPath, assetId) { mutableStateOf<File?>(null) }
    LaunchedEffect(url, localPath, assetId) {
        if (url == null && (localPath != null || assetId != null)) {
            val bytes = readAsset?.invoke(localPath ?: assetId!!)?.getOrNull()
            if (bytes != null) {
                localFile = File.createTempFile("clawseed-audio-", ".media", context.cacheDir).also {
                    it.writeBytes(bytes)
                }
            }
        }
    }
    DisposableEffect(localFile) {
        onDispose { localFile?.delete() }
    }
    val mediaUri: Uri? = url?.let(Uri::parse) ?: localFile?.let(Uri::fromFile)
    if (mediaUri == null) {
        TextUnavailable(R.string.msg_audio_unavailable, modifier)
        return
    }
    val player = remember(mediaUri) {
        ExoPlayer.Builder(context).build().apply { setMediaItem(MediaItem.fromUri(mediaUri)) }
    }
    var isPlaying by remember(player) { mutableStateOf(false) }
    var positionMs by remember(player) { mutableStateOf(0L) }
    var durationMs by remember(player) { mutableStateOf(media.durationMs ?: 0L) }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(playing: Boolean) { isPlaying = playing }
            override fun onPlaybackStateChanged(state: Int) {
                if (player.duration > 0) durationMs = player.duration
            }
        }
        player.addListener(listener)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    LaunchedEffect(player, isPlaying) {
        while (isActive && isPlaying) {
            positionMs = player.currentPosition.coerceAtLeast(0L)
            delay(500)
        }
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(ClawSeedShapes.Media)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        ClawSeedTextButton(onClick = {
            if (isPlaying) player.pause() else {
                player.prepare()
                player.play()
            }
        }) { Text(stringResource(if (isPlaying) R.string.msg_pause else R.string.msg_play)) }
        Column(modifier = Modifier.weight(1f)) {
            Text(title ?: stringResource(R.string.msg_audio_default_title), style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (durationMs > 0) {
                Text(
                    text = "${formatDuration(positionMs)} / ${formatDuration(durationMs)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

private fun formatDuration(durationMs: Long): String {
    val totalSeconds = (durationMs / 1000).coerceAtLeast(0)
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

@Composable
private fun TextUnavailable(message: Int, modifier: Modifier) {
    androidx.compose.material3.Text(
        text = stringResource(message),
        modifier = modifier.padding(top = 8.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
