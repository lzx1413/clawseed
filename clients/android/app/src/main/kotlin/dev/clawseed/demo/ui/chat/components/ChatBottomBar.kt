package dev.clawseed.demo.ui.chat.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R

@Composable
fun ChatBottomBar(
    input: String,
    onInputChange: (String) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
    isLoading: Boolean,
    canSend: Boolean,
    modifier: Modifier = Modifier,
    hasImages: Boolean = false,
    canPickImages: Boolean = true,
    onPickImages: (() -> Unit)? = null,
    imageDrafts: @Composable () -> Unit = {},
) {
    val colorScheme = MaterialTheme.colorScheme

    fun submitQuestion() {
        if ((input.isNotBlank() || hasImages) && canSend) {
            onSend()
        }
    }

    Column(modifier = modifier) {
        imageDrafts()
        OutlinedTextField(
            value = input,
            onValueChange = onInputChange,
            modifier = Modifier
                .padding(16.dp)
                .heightIn(max = 120.dp)
                .fillMaxWidth(),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = colorScheme.primary,
                unfocusedBorderColor = colorScheme.outline,
                focusedContainerColor = colorScheme.surfaceContainerLow,
                unfocusedContainerColor = colorScheme.surfaceContainerLow,
                cursorColor = colorScheme.primary,
            ),
            placeholder = {
                Text(
                    stringResource(R.string.chat_input_placeholder),
                    color = colorScheme.onSurfaceVariant,
                )
            },
            leadingIcon = onPickImages?.let { pick ->
                {
                    IconButton(onClick = pick, enabled = canSend && !isLoading && canPickImages) {
                        Icon(
                            imageVector = AttachmentIcon,
                            contentDescription = stringResource(R.string.chat_attach_image),
                            modifier = Modifier.size(24.dp),
                        )
                    }
                }
            },
            trailingIcon = {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(end = 7.dp),
                    horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp),
                ) {
                    if (isLoading) {
                        ComposerCircleButton(
                            icon = StopIcon,
                            onClick = onStop,
                            contentDescription = stringResource(R.string.chat_stop_generating),
                            isPulsing = true,
                        )
                    } else if ((input.isNotBlank() || hasImages) && canSend) {
                        ComposerCircleButton(
                            icon = SendIcon,
                            onClick = { submitQuestion() },
                            contentDescription = stringResource(R.string.chat_send),
                        )
                    }
                }
            },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Default),
            maxLines = 4,
            shape = RoundedCornerShape(28.dp),
        )

    }
}

@Composable
private fun ComposerCircleButton(
    icon: ImageVector,
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    isPulsing: Boolean = false,
) {
    val pulseModifier = if (isPulsing) {
        val infiniteTransition = rememberInfiniteTransition()
        val pulseScale by infiniteTransition.animateFloat(
            initialValue = 0.92f,
            targetValue = 1.0f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 800, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Reverse,
            ),
        )
        val pulseAlpha by infiniteTransition.animateFloat(
            initialValue = 0.7f,
            targetValue = 1.0f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 800, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Reverse,
            ),
        )
        Modifier.graphicsLayer {
            scaleX = pulseScale
            scaleY = pulseScale
            alpha = pulseAlpha
        }
    } else {
        Modifier
    }

    Box(
        modifier = modifier
            .size(42.dp)
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.primary, shape = CircleShape)
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.material3.Icon(
            imageVector = icon,
            modifier = Modifier.size(32.dp).then(pulseModifier),
            contentDescription = contentDescription,
            tint = MaterialTheme.colorScheme.onPrimary,
        )
    }
}

private val AttachmentIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "Attachment",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = androidx.compose.ui.graphics.StrokeCap.Round,
            strokeLineJoin = androidx.compose.ui.graphics.StrokeJoin.Round,
        ) {
            moveTo(21.44f, 11.05f)
            lineTo(12.25f, 20.24f)
            curveTo(9.91f, 22.58f, 6.11f, 22.58f, 3.77f, 20.24f)
            curveTo(1.43f, 17.9f, 1.43f, 14.1f, 3.77f, 11.76f)
            lineTo(12.96f, 2.57f)
            curveTo(14.52f, 1.01f, 17.06f, 1.01f, 18.62f, 2.57f)
            curveTo(20.18f, 4.13f, 20.18f, 6.67f, 18.62f, 8.23f)
            lineTo(9.42f, 17.42f)
            curveTo(8.64f, 18.2f, 7.38f, 18.2f, 6.6f, 17.42f)
            curveTo(5.82f, 16.64f, 5.82f, 15.38f, 6.6f, 14.6f)
            lineTo(15.09f, 6.12f)
        }
    }.build()
}

private val SendIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "ArrowUp",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        // Thin stroke-style up arrow: stem
        path(
            fill = SolidColor(Color.Transparent),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2.5f,
            strokeLineCap = androidx.compose.ui.graphics.StrokeCap.Round,
        ) {
            moveTo(12f, 19f)
            lineTo(12f, 5f)
        }
        // Chevron head
        path(
            fill = SolidColor(Color.Transparent),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2.5f,
            strokeLineCap = androidx.compose.ui.graphics.StrokeCap.Round,
            strokeLineJoin = androidx.compose.ui.graphics.StrokeJoin.Round,
        ) {
            moveTo(6f, 11f)
            lineTo(12f, 5f)
            lineTo(18f, 11f)
        }
    }.build()
}

private val StopIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "StopRounded",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        // Rounded rectangle stop icon (rx/ry = 2.5)
        path(fill = SolidColor(Color.Black)) {
            moveTo(8f, 6f)
            lineTo(16f, 6f)
            arcTo(2.5f, 2.5f, 0f, false, true, 18f, 8f)
            lineTo(18f, 16f)
            arcTo(2.5f, 2.5f, 0f, false, true, 16f, 18f)
            lineTo(8f, 18f)
            arcTo(2.5f, 2.5f, 0f, false, true, 6f, 16f)
            lineTo(6f, 8f)
            arcTo(2.5f, 2.5f, 0f, false, true, 8f, 6f)
            close()
        }
    }.build()
}
