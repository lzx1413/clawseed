package dev.clawseed.demo.ui.chat.components

import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import dev.clawseed.demo.ui.components.ClawSeedIconButton
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.components.GlassLevel
import dev.clawseed.demo.ui.components.GlassSurface
import androidx.compose.ui.draw.dropShadow
import dev.clawseed.demo.ui.theme.ClawSeedShadows
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import androidx.compose.ui.platform.testTag

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
    canPickFiles: Boolean = false,
    onPickFiles: (() -> Unit)? = null,
    onTakePhoto: (() -> Unit)? = null,
    onSelectRecent: ((List<android.net.Uri>) -> Unit)? = null,
    imageSlots: Int = 4,
    imageDrafts: @Composable () -> Unit = {},
) {
    var showAttachmentPicker by remember { mutableStateOf(false) }
    var inputFocused by remember { mutableStateOf(false) }
    val keyboardController = LocalSoftwareKeyboardController.current
    val colorScheme = MaterialTheme.colorScheme
    val inputShape = ClawSeedShapes.Input

    fun submitQuestion() {
        if ((input.isNotBlank() || hasImages) && canSend) {
            onSend()
        }
    }

    Column(modifier = modifier) {
        imageDrafts()
        GlassSurface(
            modifier = Modifier
                .padding(horizontal = 12.dp, vertical = 8.dp)
                .fillMaxWidth()
                .dropShadow(inputShape, ClawSeedShadows.FloatingControl),
            level = GlassLevel.Chrome,
            shape = inputShape,
            interactive = inputFocused,
        ) {
            Row(
                modifier = Modifier
                    .heightIn(min = 56.dp, max = 136.dp)
                    .fillMaxWidth()
                    .padding(horizontal = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (onPickImages != null || onPickFiles != null) {
                    ClawSeedIconButton(
                        onClick = {
                            keyboardController?.hide()
                            showAttachmentPicker = true
                        },
                        enabled = canSend && !isLoading,
                    ) {
                        Icon(
                            imageVector = AttachmentIcon,
                            contentDescription = "添加附件",
                            modifier = Modifier.size(24.dp),
                        )
                    }
                }
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .heightIn(min = 48.dp, max = 128.dp)
                        .padding(horizontal = 4.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    BasicTextField(
                        value = input,
                        onValueChange = onInputChange,
                        modifier = Modifier
                            .testTag("chat_input")
                            .fillMaxWidth()
                            .onFocusChanged { inputFocused = it.isFocused },
                        textStyle = MaterialTheme.typography.bodyLarge.merge(
                            TextStyle(color = colorScheme.onSurface),
                        ),
                        cursorBrush = SolidColor(colorScheme.primary),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Default),
                        maxLines = 4,
                        decorationBox = { innerTextField ->
                            Box(
                                modifier = Modifier.fillMaxWidth(),
                                contentAlignment = Alignment.CenterStart,
                            ) {
                                if (input.isEmpty()) {
                                    Text(
                                        stringResource(R.string.chat_input_placeholder),
                                        color = colorScheme.onSurfaceVariant,
                                        style = MaterialTheme.typography.bodyLarge,
                                    )
                                }
                                innerTextField()
                            }
                        },
                    )
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp),
                ) {
                    if (isLoading) {
                        ComposerCircleButton(
                            icon = StopIcon,
                            onClick = onStop,
                            contentDescription = stringResource(R.string.chat_stop_generating),
                            testTag = "chat_stop",
                        )
                    } else if ((input.isNotBlank() || hasImages) && canSend) {
                        ComposerCircleButton(
                            icon = SendIcon,
                            onClick = { submitQuestion() },
                            contentDescription = stringResource(R.string.chat_send),
                            testTag = "chat_send",
                        )
                    }
                }
            }
        }

    }
    if (showAttachmentPicker) {
        ChatAttachmentSheet(
            onDismiss = { showAttachmentPicker = false },
            canPickImages = canSend && !isLoading && canPickImages,
            canPickFiles = canSend && !isLoading && canPickFiles,
            imageSlots = imageSlots,
            onPickImages = onPickImages,
            onPickFiles = onPickFiles,
            onTakePhoto = onTakePhoto,
            onSelectRecent = onSelectRecent,
        )
    }

}

@Composable
private fun ComposerCircleButton(
    icon: ImageVector,
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    testTag: String? = null,
) {
    ClawSeedIconButton(
        onClick = onClick,
        modifier = modifier.then(testTag?.let { Modifier.testTag(it) } ?: Modifier),
        contentDescription = contentDescription,
        filled = true,
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(32.dp))
    }
}

internal val AttachmentIcon: ImageVector by lazy {
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
