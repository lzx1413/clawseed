package dev.clawseed.demo.ui.profile

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.R
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.ProfileCategory
import dev.clawseed.sdk.core.model.ProfileStatus
import dev.clawseed.sdk.core.model.UserProfileItem
import dev.clawseed.sdk.core.model.UserProfilePatch
import dev.clawseed.sdk.core.model.UserProfileUpsert
import dev.clawseed.sdk.core.model.MemoryEntry
import dev.clawseed.sdk.core.model.ProfileSource
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class UserProfileDraft(
    val originalId: String? = null,
    val key: String = "",
    val value: String = "",
    val category: ProfileCategory = ProfileCategory.PREFERENCE,
)

data class UserProfileUiState(
    val items: List<UserProfileItem> = emptyList(),
    val version: Long = 0,
    val isLoading: Boolean = false,
    val isSaving: Boolean = false,
    val editing: UserProfileDraft? = null,
    val error: String? = null,
    val tab: KnowledgeTab = KnowledgeTab.ABOUT_ME,
    val memories: List<MemoryEntry> = emptyList(),
    val selectedIds: Set<String> = emptySet(),
    val categoryFilter: ProfileCategory? = null,
    val sourceFilter: ProfileSource? = null,
    val statusFilter: ProfileStatus? = null,
)

enum class KnowledgeTab { ABOUT_ME, MEMORY }

class UserProfileViewModel(application: Application) : AndroidViewModel(application) {
    private val _uiState = MutableStateFlow(UserProfileUiState())
    val uiState: StateFlow<UserProfileUiState> = _uiState.asStateFlow()

    fun load() {
        viewModelScope.launch {
            if (!ClawSeedAndroid.isInitialized) return@launch
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)
            ClawSeedAndroid.gatewayClient().userProfile()
                .onSuccess { profile ->
                    _uiState.value = _uiState.value.copy(
                        items = profile.items.sortedProfileItems(),
                        version = profile.version,
                        isLoading = false,
                    )
                }
                .onFailure { error ->
                    _uiState.value = _uiState.value.copy(
                        isLoading = false,
                        error = error.message,
                    )
                }
        }
    }

    fun selectTab(tab: KnowledgeTab) {
        _uiState.value = _uiState.value.copy(tab = tab, selectedIds = emptySet())
        if (tab == KnowledgeTab.MEMORY) loadMemories()
    }

    fun loadMemories() {
        viewModelScope.launch {
            if (!ClawSeedAndroid.isInitialized) return@launch
            _uiState.value = _uiState.value.copy(isLoading = true, error = null)
            ClawSeedAndroid.gatewayClient().memories()
                .onSuccess { result ->
                    _uiState.value = _uiState.value.copy(
                        memories = result.entries,
                        isLoading = false,
                    )
                }
                .onFailure { error ->
                    _uiState.value = _uiState.value.copy(isLoading = false, error = error.message)
                }
        }
    }

    fun setCategoryFilter(value: ProfileCategory?) {
        _uiState.value = _uiState.value.copy(categoryFilter = value, selectedIds = emptySet())
    }

    fun setSourceFilter(value: ProfileSource?) {
        _uiState.value = _uiState.value.copy(sourceFilter = value, selectedIds = emptySet())
    }

    fun setStatusFilter(value: ProfileStatus?) {
        _uiState.value = _uiState.value.copy(statusFilter = value, selectedIds = emptySet())
    }

    fun toggleSelection(itemId: String) {
        val selected = _uiState.value.selectedIds.toMutableSet()
        if (!selected.add(itemId)) selected.remove(itemId)
        _uiState.value = _uiState.value.copy(selectedIds = selected)
    }

    fun clearSelection() {
        _uiState.value = _uiState.value.copy(selectedIds = emptySet())
    }

    fun deleteSelected() = deleteItems(_uiState.value.selectedIds)

    fun rejectSelected() = rejectItems(_uiState.value.selectedIds)

    fun deleteAllInferred() {
        val ids = _uiState.value.items
            .filter { it.source == ProfileSource.INFERRED }
            .map { it.id }
        deleteItems(ids)
    }

    private fun deleteItems(itemIds: Collection<String>) {
        if (itemIds.isEmpty()) return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            val deleted = mutableSetOf<String>()
            var failure: Throwable? = null
            for (itemId in itemIds.sorted()) {
                ClawSeedAndroid.gatewayClient().deleteUserProfileItem(itemId)
                    .onSuccess { deleted += itemId }
                    .onFailure { error -> if (failure == null) failure = error }
            }
            _uiState.value = _uiState.value.copy(
                items = _uiState.value.items.filterNot { it.id in deleted },
                selectedIds = emptySet(),
                isSaving = false,
                error = failure?.message,
            )
        }
    }

    fun create() {
        _uiState.value = _uiState.value.copy(editing = UserProfileDraft(), error = null)
    }

    fun edit(item: UserProfileItem) {
        _uiState.value = _uiState.value.copy(
            editing = UserProfileDraft(
                originalId = item.id,
                key = item.key,
                value = UserProfileValueCodec.display(item.value),
                category = item.category,
            ),
            error = null,
        )
    }

    fun updateDraft(transform: (UserProfileDraft) -> UserProfileDraft) {
        val draft = _uiState.value.editing ?: return
        _uiState.value = _uiState.value.copy(editing = transform(draft), error = null)
    }

    fun closeEditor() {
        _uiState.value = _uiState.value.copy(editing = null)
    }

    fun save() {
        val draft = _uiState.value.editing ?: return
        val key = draft.key.trim()
        if (!isUserProfileKeyValid(key)) {
            _uiState.value = _uiState.value.copy(
                error = getApplication<Application>().getString(R.string.profile_key_invalid),
            )
            return
        }
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            val value = UserProfileValueCodec.parse(draft.value)
            val result = if (draft.originalId == null) {
                ClawSeedAndroid.gatewayClient().upsertUserProfileItem(
                    UserProfileUpsert(key = key, value = value, category = draft.category),
                )
            } else {
                ClawSeedAndroid.gatewayClient().patchUserProfileItem(
                    draft.originalId,
                    UserProfilePatch(value = value, category = draft.category),
                )
            }
            result.onSuccess { item ->
                val items = _uiState.value.items
                    .filterNot { it.id == item.id || it.key == item.key }
                    .plus(item)
                    .sortedProfileItems()
                _uiState.value = _uiState.value.copy(
                    items = items,
                    version = maxOf(_uiState.value.version, item.version),
                    isSaving = false,
                    editing = null,
                )
            }.onFailure { error ->
                _uiState.value = _uiState.value.copy(isSaving = false, error = error.message)
            }
        }
    }

    fun reject(item: UserProfileItem) {
        rejectItems(listOf(item.id))
    }

    private fun rejectItems(itemIds: Collection<String>) {
        if (itemIds.isEmpty()) return
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            val updates = mutableMapOf<String, UserProfileItem>()
            var failure: Throwable? = null
            for (itemId in itemIds.sorted()) {
                ClawSeedAndroid.gatewayClient().patchUserProfileItem(
                    itemId,
                    UserProfilePatch(status = ProfileStatus.REJECTED),
                ).onSuccess { updated -> updates[itemId] = updated }
                    .onFailure { error -> if (failure == null) failure = error }
            }
            _uiState.value = _uiState.value.copy(
                items = _uiState.value.items
                    .map { updates[it.id] ?: it }
                    .sortedProfileItems(),
                version = maxOf(_uiState.value.version, updates.values.maxOfOrNull { it.version } ?: 0),
                selectedIds = emptySet(),
                isSaving = false,
                error = failure?.message,
            )
        }
    }

    fun delete(item: UserProfileItem) {
        deleteItems(listOf(item.id))
    }

    fun clear() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isSaving = true, error = null)
            ClawSeedAndroid.gatewayClient().clearUserProfile()
                .onSuccess {
                    _uiState.value = _uiState.value.copy(
                        items = emptyList(),
                        isSaving = false,
                        editing = null,
                    )
                }
                .onFailure { error ->
                    _uiState.value = _uiState.value.copy(isSaving = false, error = error.message)
                }
        }
    }

    fun clearError() {
        _uiState.value = _uiState.value.copy(error = null)
    }

}

fun isUserProfileKeyValid(key: String): Boolean =
    key.isNotEmpty() && key.length <= 256 && key.all { it.isLetterOrDigit() || it in "._-" } && key.all { it.code < 128 }

private fun List<UserProfileItem>.sortedProfileItems(): List<UserProfileItem> =
    sortedWith(compareBy<UserProfileItem> { it.category.ordinal }.thenBy { it.key })
