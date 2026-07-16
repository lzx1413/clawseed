package dev.clawseed.demo.ui.settings

internal object UserModelConfigToml {
    private const val SECTION_HEADER = "[user_model]"

    fun extractAutoInfer(toml: String): Boolean {
        val lines = toml.lines()
        val sectionStart = lines.indexOfFirst { it.trim() == SECTION_HEADER }
        if (sectionStart == -1) return false
        val sectionEnd = findSectionEnd(lines, sectionStart)
        val enabled = extractBoolean(lines, sectionStart + 1, sectionEnd, "enabled") ?: true
        val autoInfer = extractBoolean(lines, sectionStart + 1, sectionEnd, "auto_infer") ?: false
        return enabled && autoInfer
    }

    fun updateAutoInfer(toml: String, autoInfer: Boolean): String {
        val hadTrailingNewline = toml.endsWith('\n')
        val lines = toml.lines().toMutableList().apply {
            if (hadTrailingNewline && isNotEmpty() && last().isEmpty()) removeAt(lastIndex)
        }
        val sectionStart = lines.indexOfFirst { it.trim() == SECTION_HEADER }
        if (sectionStart == -1) {
            val prefix = toml.trimEnd()
            return buildString {
                if (prefix.isNotEmpty()) {
                    append(prefix)
                    append("\n\n")
                }
                appendLine(SECTION_HEADER)
                appendLine("enabled = true")
                appendLine("max_prompt_items = 20")
                appendLine("auto_infer = $autoInfer")
                appendLine("inference_min_confidence = 0.8")
                appendLine("max_inferred_items_per_turn = 3")
            }
        }

        var sectionEnd = findSectionEnd(lines, sectionStart)
        if (autoInfer) {
            sectionEnd = setBoolean(lines, sectionStart + 1, sectionEnd, "enabled", true)
        }
        setBoolean(lines, sectionStart + 1, sectionEnd, "auto_infer", autoInfer)
        return lines.joinToString("\n") + if (hadTrailingNewline) "\n" else ""
    }

    private fun findSectionEnd(lines: List<String>, sectionStart: Int): Int {
        val nextSection = (sectionStart + 1 until lines.size).firstOrNull { index ->
            lines[index].trimStart().startsWith('[')
        }
        return nextSection ?: lines.size
    }

    private fun extractBoolean(
        lines: List<String>,
        start: Int,
        end: Int,
        key: String,
    ): Boolean? {
        for (index in start until end) {
            val parsed = parseAssignment(lines[index]) ?: continue
            if (parsed.first == key) return parsed.second.toBooleanStrictOrNull()
        }
        return null
    }

    private fun setBoolean(
        lines: MutableList<String>,
        start: Int,
        end: Int,
        key: String,
        value: Boolean,
    ): Int {
        for (index in start until end) {
            val parsed = parseAssignment(lines[index]) ?: continue
            if (parsed.first != key) continue
            val line = lines[index]
            val equalsIndex = line.indexOf('=')
            val comment = line.substring(equalsIndex + 1).indexOf('#').let { offset ->
                if (offset == -1) "" else " " + line.substring(equalsIndex + 1 + offset).trimStart()
            }
            lines[index] = line.substring(0, equalsIndex + 1) + " $value" + comment
            return end
        }
        var insertionIndex = end
        while (insertionIndex > start && lines[insertionIndex - 1].isBlank()) {
            insertionIndex--
        }
        lines.add(insertionIndex, "$key = $value")
        return end + 1
    }

    private fun parseAssignment(line: String): Pair<String, String>? {
        val content = line.substringBefore('#').trim()
        val equalsIndex = content.indexOf('=')
        if (equalsIndex <= 0) return null
        val key = content.substring(0, equalsIndex).trim()
        val value = content.substring(equalsIndex + 1).trim()
        return key to value
    }
}
