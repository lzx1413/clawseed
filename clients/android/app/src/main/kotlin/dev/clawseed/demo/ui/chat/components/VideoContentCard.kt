package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.unit.dp
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.sdk.core.model.MediaReference

@Composable
internal fun VideoContentCard(
    media: MediaReference,
    title: String?,
    modifier: Modifier = Modifier,
) {
    val url = media.safeHttpsUrl()
    val context = androidx.compose.ui.platform.LocalContext.current
    if (url == null) {
        Text(
            text = androidx.compose.ui.res.stringResource(R.string.msg_video_unavailable),
            modifier = modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val player = remember(url) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            prepare()
            playWhenReady = false
        }
    }
    DisposableEffect(player) { onDispose { player.release() } }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(ClawSeedShapes.Media)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        title?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
        AndroidView(
            factory = { viewContext ->
                PlayerView(viewContext).apply {
                    this.player = player
                    useController = true
                }
            },
            update = { it.player = player },
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(16f / 9f)
                .clip(ClawSeedShapes.Content),
        )
    }
}
