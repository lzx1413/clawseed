package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R

@Composable
fun ChatBottomBar(
    input: String,
    onInputChange: (String) -> Unit,
    onSend: () -> Unit,
    onStop: (() -> Unit)? = null,
    canSend: Boolean,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(
            value = input,
            onValueChange = onInputChange,
            placeholder = { Text(stringResource(R.string.chat_input_placeholder)) },
            modifier = Modifier
                .weight(1f)
                .padding(end = 8.dp),
            shape = RoundedCornerShape(24.dp),
            maxLines = 4,
        )
        if (onStop != null) {
            IconButton(
                onClick = onStop,
                colors = IconButtonDefaults.iconButtonColors(),
            ) {
                Icon(Icons.Default.Close, contentDescription = stringResource(R.string.chat_stop_generating))
            }
        } else {
            IconButton(
                onClick = onSend,
                enabled = canSend && input.isNotBlank(),
            ) {
                Icon(Icons.Default.Send, contentDescription = stringResource(R.string.chat_send))
            }
        }
    }
}
