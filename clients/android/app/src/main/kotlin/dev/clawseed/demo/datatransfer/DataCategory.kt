package dev.clawseed.demo.datatransfer

/** Categories of data that can be exported/imported. */
enum class DataCategory(
    val label: String,
    val description: String,
    val isSensitive: Boolean,
) {
    CONFIG(
        label = "配置",
        description = "导出 clawseed.toml、应用偏好、定时任务",
        isSensitive = true,
    ),
    MEMORY(
        label = "记忆",
        description = "导出记忆数据库 (brain.db)",
        isSensitive = false,
    ),
    SESSIONS(
        label = "会话",
        description = "导出会话历史记录",
        isSensitive = false,
    ),
    SKILLS(
        label = "技能",
        description = "导出所有技能文件和配置",
        isSensitive = false,
    ),
    PERSONALITY(
        label = "人格",
        description = "导出 SOUL.md 等人格文件",
        isSensitive = false,
    ),
}
