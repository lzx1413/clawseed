package dev.clawseed.demo.ui.chat.components

import android.content.res.Configuration
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import dev.clawseed.demo.ui.theme.ClawSeedTheme
import dev.clawseed.sdk.core.model.ChatEvent

data class AskUserPreviewState(
    val kind: String,
    val submitting: Boolean = false,
    val enabled: Boolean = true,
)

class AskUserPreviewProvider : PreviewParameterProvider<AskUserPreviewState> {
    override val values = sequenceOf(
        AskUserPreviewState("single_select"),
        AskUserPreviewState("multi_select"),
        AskUserPreviewState("text"),
        AskUserPreviewState("confirm"),
        AskUserPreviewState("confirm", submitting = true),
        AskUserPreviewState("multi_select", enabled = false),
    )
}

@Preview(name = "Ask User light", widthDp = 360, locale = "zh-rCN", showBackground = true)
@Preview(name = "Ask User dark", widthDp = 360, locale = "zh-rCN", showBackground = true,
    uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
private fun AskUserPromptPreview(@PreviewParameter(AskUserPreviewProvider::class) state: AskUserPreviewState) {
    ClawSeedTheme(dark = isSystemInDarkTheme()) {
        Surface {
            AskUserPrompt(
                request = ChatEvent.AskUserRequested(
                    requestId = "preview",
                    sessionId = "preview",
                    turnId = "preview",
                    toolCallId = "preview",
                    kind = state.kind,
                    question = when (state.kind) {
                        "text" -> "你希望补充哪些细节？"
                        "confirm" -> "是否继续整理这份计划？"
                        else -> "你希望如何安排下一步？"
                    },
                    options = listOf(
                        ChatEvent.AskUserOption("summary", "先看摘要", "先了解主要内容，再决定下一步。"),
                        ChatEvent.AskUserOption("detail", "展开详细步骤", "查看每个步骤的说明和需要准备的材料。"),
                    ),
                    placeholder = "输入补充说明",
                ),
                submitting = state.submitting,
                enabled = state.enabled,
                onAnswer = { _, _ -> },
            )
        }
    }
}
