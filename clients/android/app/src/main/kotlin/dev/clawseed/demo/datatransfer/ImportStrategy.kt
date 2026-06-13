package dev.clawseed.demo.datatransfer

/** Strategy for importing data into an existing data set. */
enum class ImportStrategy(val label: String) {
    /** Replace all existing data with imported data. */
    REPLACE(label = "替换"),
    /** Merge imported data with existing — keep existing, add new, overwrite same-name entries. */
    MERGE(label = "合并"),
    /** Append imported data alongside existing — never overwrite existing entries. */
    APPEND(label = "追加"),
}
