package dev.clawseed.demo.datatransfer

import androidx.annotation.StringRes
import dev.clawseed.demo.R

/** Categories of data that can be exported/imported. */
enum class DataCategory(
    @param:StringRes val labelRes: Int,
    @param:StringRes val descriptionRes: Int,
    val isSensitive: Boolean,
) {
    CONFIG(
        labelRes = R.string.enum_data_category_config,
        descriptionRes = R.string.enum_data_category_config_desc,
        isSensitive = true,
    ),
    MEMORY(
        labelRes = R.string.enum_data_category_memory,
        descriptionRes = R.string.enum_data_category_memory_desc,
        isSensitive = false,
    ),
    SESSIONS(
        labelRes = R.string.enum_data_category_sessions,
        descriptionRes = R.string.enum_data_category_sessions_desc,
        isSensitive = false,
    ),
    SKILLS(
        labelRes = R.string.enum_data_category_skills,
        descriptionRes = R.string.enum_data_category_skills_desc,
        isSensitive = false,
    ),
    USER_PROFILE(
        labelRes = R.string.enum_data_category_user_profile,
        descriptionRes = R.string.enum_data_category_user_profile_desc,
        isSensitive = true,
    ),
    PERSONALITY(
        labelRes = R.string.enum_data_category_personality,
        descriptionRes = R.string.enum_data_category_personality_desc,
        isSensitive = false,
    ),
}
