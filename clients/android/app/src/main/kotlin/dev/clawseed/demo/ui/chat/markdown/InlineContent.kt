package dev.clawseed.demo.ui.chat.markdown

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign

/**
 * Render a list of [InlineNode]s as a single [Text] composable with [AnnotatedString].
 *
 * Simplified from Kai's version (which had math formula support with FlowRow).
 * Since ClawSeed doesn't need math rendering, this always uses the plain Text path,
 * preserving native text selection, word wrapping, and alignment.
 */
@Composable
internal fun InlineContent(
    inlines: List<InlineNode>,
    style: TextStyle,
    modifier: Modifier = Modifier,
    textAlign: TextAlign = TextAlign.Unspecified,
) {
    Text(
        text = inlines.toAnnotatedString(),
        style = style,
        textAlign = textAlign,
        modifier = modifier,
    )
}
