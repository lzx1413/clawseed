package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.*
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.theme.*

enum class ClawSeedButtonStyle { Primary, Outlined, Text }

@Composable
fun ClawSeedButton(
    onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    loading: Boolean = false, dangerous: Boolean = false,
    style: ClawSeedButtonStyle = ClawSeedButtonStyle.Primary,
    contentPadding: PaddingValues = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
    content: @Composable RowScope.() -> Unit,
) {
    val scheme = MaterialTheme.colorScheme
    val loadingLabel = stringResource(R.string.common_loading)
    val buttonModifier = modifier.heightIn(min = ClawSeedSizes.Button).semantics {
        if (loading) stateDescription = loadingLabel
    }
    val body: @Composable RowScope.() -> Unit = {
        Box(contentAlignment = Alignment.Center) {
            Row(Modifier.graphicsLayer { alpha = if (loading) 0f else 1f },
                verticalAlignment = Alignment.CenterVertically, content = content)
            if (loading) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        }
    }
    when (style) {
        ClawSeedButtonStyle.Primary -> Button(onClick, buttonModifier, enabled && !loading,
            shape = ClawSeedShapes.Field, contentPadding = contentPadding,
            colors = ButtonDefaults.buttonColors(
                containerColor = if (dangerous) scheme.error else scheme.primary,
                contentColor = if (dangerous) scheme.onError else scheme.onPrimary), content = body)
        ClawSeedButtonStyle.Outlined -> OutlinedButton(onClick, buttonModifier, enabled && !loading,
            shape = ClawSeedShapes.Field, contentPadding = contentPadding,
            colors = ButtonDefaults.outlinedButtonColors(contentColor = if (dangerous) scheme.error else scheme.primary), content = body)
        ClawSeedButtonStyle.Text -> TextButton(onClick, buttonModifier, enabled && !loading,
            shape = ClawSeedShapes.Field, contentPadding = contentPadding,
            colors = ButtonDefaults.textButtonColors(contentColor = if (dangerous) scheme.error else scheme.primary), content = body)
    }
}

@Composable
fun ClawSeedOutlinedButton(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    loading: Boolean = false, dangerous: Boolean = false,
    contentPadding: PaddingValues = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
    content: @Composable RowScope.() -> Unit) = ClawSeedButton(onClick, modifier, enabled, loading,
    dangerous, ClawSeedButtonStyle.Outlined, contentPadding, content)

@Composable
fun ClawSeedTextButton(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    loading: Boolean = false, dangerous: Boolean = false,
    contentPadding: PaddingValues = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
    content: @Composable RowScope.() -> Unit) = ClawSeedButton(onClick, modifier, enabled, loading,
    dangerous, ClawSeedButtonStyle.Text, contentPadding, content)
