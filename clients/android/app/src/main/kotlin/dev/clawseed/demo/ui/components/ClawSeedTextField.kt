package dev.clawseed.demo.ui.components

import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.VisualTransformation
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.rowTitle

@Composable
fun ClawSeedTextField(value: String, onValueChange: (String) -> Unit, modifier: Modifier = Modifier,
    enabled: Boolean = true, readOnly: Boolean = false, textStyle: TextStyle = MaterialTheme.typography.rowTitle,
    label: (@Composable () -> Unit)? = null, placeholder: (@Composable () -> Unit)? = null,
    leadingIcon: (@Composable () -> Unit)? = null, trailingIcon: (@Composable () -> Unit)? = null,
    supportingText: (@Composable () -> Unit)? = null, prefix: (@Composable () -> Unit)? = null,
    suffix: (@Composable () -> Unit)? = null, isError: Boolean = false,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default, singleLine: Boolean = false,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE, minLines: Int = 1,
    glass: Boolean = false,
    shape: Shape = ClawSeedShapes.Field,
) {
    val colors = MaterialTheme.colorScheme
    OutlinedTextField(value, onValueChange, modifier, enabled, readOnly, textStyle,
        label, placeholder, leadingIcon, trailingIcon, prefix, suffix, supportingText, isError,
        visualTransformation, keyboardOptions, keyboardActions, singleLine, maxLines, minLines,
        shape = shape,
        colors = OutlinedTextFieldDefaults.colors(
            focusedContainerColor = if (glass) colors.surface.copy(alpha = 0.36f) else colors.surfaceContainerLow,
            unfocusedContainerColor = if (glass) colors.surface.copy(alpha = 0.24f) else colors.surfaceContainerLow,
            disabledContainerColor = if (glass) colors.surface.copy(alpha = 0.18f) else colors.surfaceContainerLow,
            errorContainerColor = if (glass) colors.surface.copy(alpha = 0.36f) else colors.surfaceContainerLow,
            unfocusedBorderColor = if (glass) colors.outlineVariant.copy(alpha = 0.5f) else colors.outline,
            focusedBorderColor = colors.primary, errorBorderColor = colors.error))
}
