package dev.clawseed.demo.datatransfer

/** Summary of an import operation. */
data class ImportResult(
    val importedSkills: Int = 0,
    val importedMemories: Int = 0,
    val importedSessions: Int = 0,
    val importedMessages: Int = 0,
    val importedPersonalityFiles: Int = 0,
    val importedConfig: Boolean = false,
    val importedTasks: Int = 0,
    val warnings: List<String> = emptyList(),
    val errors: List<String> = emptyList(),
) {
    val isSuccess: Boolean get() = errors.isEmpty()

    val summary: String
        get() = buildString {
            val parts = mutableListOf<String>()
            if (importedConfig) parts.add("配置已恢复")
            if (importedTasks > 0) parts.add("${importedTasks} 个定时任务")
            if (importedMemories > 0) parts.add("${importedMemories} 条记忆")
            if (importedSessions > 0) parts.add("${importedSessions} 个会话")
            if (importedMessages > 0) parts.add("${importedMessages} 条消息")
            if (importedSkills > 0) parts.add("${importedSkills} 个技能")
            if (importedPersonalityFiles > 0) parts.add("${importedPersonalityFiles} 个人格文件")
            append(parts.joinToString(", "))
        }
}
