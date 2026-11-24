package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import coil3.compose.AsyncImage
import coil3.compose.rememberAsyncImagePainter
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.chat.rememberRichMediaImageLoader
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.sdk.core.model.MediaReference

internal fun MediaReference.safeHttpsUrl(): String? = url?.takeIf { it.startsWith("https://") }

@Composable
internal fun ImageContentCard(
    media: MediaReference,
    alt: String?,
    readAsset: (suspend (String) -> Result<ByteArray>)?,
    modifier: Modifier = Modifier,
) {
    val url = media.safeHttpsUrl()
    val assetId = media.assetId?.takeIf { it.isNotBlank() }
    val localPath = media.path?.takeIf { it.isNotBlank() }
    val imageLoader = rememberRichMediaImageLoader()
    var assetResult by remember(localPath, assetId) { mutableStateOf<Result<ByteArray>?>(null) }
    var showPreview by remember(url, localPath, assetId) { mutableStateOf(false) }
    val colorScheme = MaterialTheme.colorScheme

    LaunchedEffect(assetId, localPath, url) {
        if (url == null && (localPath != null || assetId != null)) {
            val source = localPath ?: assetId
            assetResult = readAsset?.invoke(source!!)
                ?: Result.failure(IllegalStateException("Image attachment reader unavailable"))
        }
    }

    val imageModel: Any? = url ?: assetResult?.getOrNull()

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(ClawSeedShapes.Media)
            .background(colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (imageModel == null && (localPath != null || assetId != null) && assetResult == null) {
            CircularProgressIndicator(modifier = Modifier.size(24.dp))
        } else if (imageModel == null) {
            Text(
                text = stringResource(R.string.msg_image_unavailable),
                style = MaterialTheme.typography.bodySmall,
                color = colorScheme.onSurfaceVariant,
            )
        } else {
            AsyncImage(
                model = imageModel,
                imageLoader = imageLoader,
                contentDescription = alt ?: stringResource(R.string.msg_image_attachment),
                contentScale = ContentScale.Crop,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 260.dp)
                    .clip(ClawSeedShapes.Content)
                    .clickable { showPreview = true },
            )
            alt?.takeIf { it.isNotBlank() }?.let { description ->
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = colorScheme.onSurfaceVariant,
                )
            }
        }
    }

    if (showPreview && imageModel != null) {
        Dialog(onDismissRequest = { showPreview = false }) {
            Image(
                painter = rememberAsyncImagePainter(
                    model = imageModel,
                    imageLoader = imageLoader,
                ),
                contentDescription = alt ?: stringResource(R.string.msg_image_attachment),
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(1f),
            )
        }
    }
}
