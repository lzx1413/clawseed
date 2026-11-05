package dev.clawseed.demo.ui.components

import android.content.res.Configuration
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedTheme

@Preview(name = "Light", showBackground = true, widthDp = 360)
@Preview(name = "Dark", showBackground = true, widthDp = 360, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Preview(name = "Narrow large text", showBackground = true, widthDp = 320, fontScale = 2f)
@Composable
private fun ComponentPreview() {
    ClawSeedTheme(dark = androidx.compose.foundation.isSystemInDarkTheme()) {
        Surface { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            SettingsGroup { Column(Modifier.padding(16.dp)) {
                SettingsSwitchRow("语音输出 Voice output", true, {}, subtitle = "朗读回复 Read responses aloud")
                SettingsCheckboxRow("配置 Configuration", true, {}, subtitle = "导出应用偏好 Export app preferences")
                SettingsRow("模型 Model", value = "Long model name / 长名称模型", onClick = {})
            } }
            ClawSeedTextField("", {}, label = { Text("API key") }, isError = true, supportingText = { Text("Required / 必填") })
            ClawSeedSegmentedPill(listOf("浅色 Light", "深色 Dark", "系统 System"), 0, {})
            ClawSeedButton({}, loading = true) { Text("保存 Save") }
            ClawSeedOutlinedButton({}, enabled = false) { Text("Disabled") }
            ClawSeedTextButton({}, dangerous = true) { Text("Delete") }
        } }
    }
}
