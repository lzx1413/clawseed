package dev.clawseed.demo.datatransfer

import androidx.annotation.StringRes
import dev.clawseed.demo.R

/** Strategy for importing data into an existing data set. */
enum class ImportStrategy(@StringRes val labelRes: Int) {
    /** Replace all existing data with imported data. */
    REPLACE(labelRes = R.string.enum_import_strategy_replace),
    /** Merge imported data with existing — keep existing, add new, overwrite same-name entries. */
    MERGE(labelRes = R.string.enum_import_strategy_merge),
    /** Append imported data alongside existing — never overwrite existing entries. */
    APPEND(labelRes = R.string.enum_import_strategy_append),
}
