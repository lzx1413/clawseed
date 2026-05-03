package dev.clawseed.demo.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "clawseed_prefs")

class LocalStore(private val context: Context) {

    private val store get() = context.dataStore

    // --- Active session ID ---
    private val KEY_SESSION_ID = stringPreferencesKey("active_session_id")

    val activeSessionId: Flow<String?> = store.data.map { it[KEY_SESSION_ID] }

    suspend fun setActiveSessionId(id: String?) {
        store.edit { prefs ->
            if (id != null) prefs[KEY_SESSION_ID] = id else prefs.remove(KEY_SESSION_ID)
        }
    }

    // --- Draft message ---
    private val KEY_DRAFT = stringPreferencesKey("draft_message")

    val draftMessage: Flow<String> = store.data.map { it[KEY_DRAFT] ?: "" }

    suspend fun setDraftMessage(text: String) {
        store.edit { prefs ->
            if (text.isNotEmpty()) prefs[KEY_DRAFT] = text else prefs.remove(KEY_DRAFT)
        }
    }

    // --- Bearer token ---
    private val KEY_BEARER_TOKEN = stringPreferencesKey("bearer_token")

    val bearerToken: Flow<String?> = store.data.map { it[KEY_BEARER_TOKEN] }

    suspend fun setBearerToken(token: String?) {
        store.edit { prefs ->
            if (token != null) prefs[KEY_BEARER_TOKEN] = token else prefs.remove(KEY_BEARER_TOKEN)
        }
    }

    // --- Debug mode ---
    private val KEY_SHOW_DEBUG = booleanPreferencesKey("show_debug_info")

    val showDebugInfo: Flow<Boolean> = store.data.map { it[KEY_SHOW_DEBUG] ?: false }

    suspend fun setShowDebugInfo(enabled: Boolean) {
        store.edit { prefs -> prefs[KEY_SHOW_DEBUG] = enabled }
    }
}
