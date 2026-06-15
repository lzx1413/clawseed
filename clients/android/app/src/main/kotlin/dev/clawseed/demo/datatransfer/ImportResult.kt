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
    // summary is now resolved in Composable context via ImportResultSummary composable
}
