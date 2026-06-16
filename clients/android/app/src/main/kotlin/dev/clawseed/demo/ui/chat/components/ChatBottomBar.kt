package dev.clawseed.demo.ui.chat.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.platform.LocalInspectionMode
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
) {
    val colorScheme = MaterialTheme.colorScheme
    val gradientBrush = remember(colorScheme.primary, colorScheme.tertiary) {
        Brush.horizontalGradient(listOf(colorScheme.primary, colorScheme.tertiary))
    }

    fun submitQuestion() {
        if (input.isNotBlank() && canSend) {
            onSend()
        }
    }

    val focusRequester = remember { FocusRequester() }
    val inInspection = LocalInspectionMode.current

    Column(modifier = modifier) {
        OutlinedTextField(
            value = input,
            onValueChange = onInputChange,
            modifier = Modifier
                .focusRequester(focusRequester)
                .padding(16.dp)
                .heightIn(max = 120.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(28.dp))
                .background(colorScheme.background)
                .border(
                    BorderStroke(width = 2.dp, brush = gradientBrush),
                    shape = RoundedCornerShape(28.dp),
                ),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Color.Transparent,
                unfocusedBorderColor = Color.Transparent,
                disabledBorderColor = Color.Transparent,
                focusedContainerColor = Color.Transparent,
                unfocusedContainerColor = Color.Transparent,
            ),
            placeholder = {
                Text(
                    stringResource(R.string.chat_input_placeholder),
                    color = colorScheme.onSurfaceVariant,
                )
            },
            trailingIcon = {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(end = 7.dp),
                    horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp),
                ) {
                    if (isLoading) {
                        GradientCircleButton(
                            icon = StopIcon,
                            onClick = onStop,
                            gradientBrush = gradientBrush,
                            contentDescription = stringResource(R.string.chat_stop_generating),
                            isPulsing = true,
                        )
                    } else if (input.isNotBlank() && canSend) {
                        GradientCircleButton(
                            icon = SendIcon,
                            onClick = { submitQuestion() },
                            gradientBrush = gradientBrush,
                            contentDescription = stringResource(R.string.chat_send),
                        )
                    }
                }
            },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Default),
            maxLines = 4,
            shape = RoundedCornerShape(28.dp),
        )

        // Auto-focus on first composition (opens keyboard on chat entry)
        LaunchedEffect(Unit) {
            if (!inInspection) focusRequester.requestFocus()
        }
    }
}

@Composable
private fun GradientCircleButton(
    icon: ImageVector,
    onClick: () -> Unit,
    gradientBrush: Brush,
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
            .background(brush = gradientBrush, shape = CircleShape)
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.material3.Icon(
            imageVector = icon,
            modifier = Modifier.size(32.dp).then(pulseModifier),
            contentDescription = contentDescription,
            tint = Color.White,
        )
    }
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
