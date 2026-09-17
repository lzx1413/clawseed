package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.GatewayStatus
import dev.clawseed.sdk.core.model.HealthInfo
import dev.clawseed.sdk.core.model.PersonaDetail
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.PersonaUpsert
import dev.clawseed.sdk.core.model.ProviderInfo
import dev.clawseed.sdk.core.model.SessionMessage
import dev.clawseed.sdk.core.model.SessionSummary
import dev.clawseed.sdk.core.model.SkillDetail
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.ToolInfo
import dev.clawseed.sdk.core.model.UserProfile
import dev.clawseed.sdk.core.model.UserProfileImportItem
import dev.clawseed.sdk.core.model.UserProfileImportRequest
import dev.clawseed.sdk.core.model.UserProfileImportResult
import dev.clawseed.sdk.core.model.UserProfileItem
import dev.clawseed.sdk.core.model.UserProfilePatch
import dev.clawseed.sdk.core.model.UserProfileUpsert
import dev.clawseed.sdk.core.model.UserProfileSearchRequest
import dev.clawseed.sdk.core.model.UserProfileSearchResult
import dev.clawseed.sdk.core.model.UserProfileChangePlanRequest
import dev.clawseed.sdk.core.model.UserProfileChangePlan
import dev.clawseed.sdk.core.model.UserProfileMutationResult
import dev.clawseed.sdk.core.model.MemoryEntries
import dev.clawseed.sdk.core.model.ProfileImportStrategy
import dev.clawseed.sdk.core.model.WebhookResponse
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/**
 * Lightweight REST client for the ClawSeed gateway HTTP API.
 */
class GatewayClient(
    /** Base HTTP URL for the target gateway. */
    val baseUrl: String,
    /** Supplies the bearer token used for authenticated requests. */
    val authTokenProvider: () -> String? = { null },
    httpClient: OkHttpClient = defaultHttpClient(),
) {
    /** Convenience constructor for callers that use a fixed bearer token. */
    constructor(
        baseUrl: String,
        authToken: String?,
        httpClient: OkHttpClient = defaultHttpClient(),
    ) : this(baseUrl, { authToken }, httpClient)

    private val client = httpClient
    private val json = Json { ignoreUnknownKeys = true }

    private fun Request.Builder.addAuth(): Request.Builder {
        authTokenProvider()?.let { addHeader("Authorization", "Bearer $it") }
        return this
    }

    private suspend fun execute(request: Request): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val resp = client.newCall(request).execute()
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw Exception("HTTP ${resp.code}: $body")
            body
        }
    }

    private fun imageUrl(sessionId: String, attachmentId: String? = null): okhttp3.HttpUrl {
        val builder = baseUrl.trimEnd('/').toHttpUrl().newBuilder()
            .addPathSegment("api").addPathSegment("sessions").addPathSegment(sessionId)
            .addPathSegment("attachments")
        attachmentId?.let(builder::addPathSegment)
        return builder.build()
    }

    suspend fun uploadImage(sessionId: String, bytes: ByteArray): Result<dev.clawseed.sdk.core.model.ImageAttachment> = withContext(Dispatchers.IO) {
        runCatching {
            require(bytes.isNotEmpty() && bytes.size <= 5 * 1024 * 1024) { "Image must not exceed 5 MiB" }
            val request = Request.Builder().url(imageUrl(sessionId)).addAuth()
                .post(bytes.toRequestBody("application/octet-stream".toMediaType())).build()
            json.decodeFromString<dev.clawseed.sdk.core.model.ImageAttachment>(execute(request).getOrThrow())
        }
    }

    suspend fun readImage(sessionId: String, attachmentId: String): Result<ByteArray> = withContext(Dispatchers.IO) {
        runCatching {
            val request = Request.Builder().url(imageUrl(sessionId, attachmentId)).addAuth().build()
            client.newCall(request).execute().use { response ->
                check(response.isSuccessful) { "Image unavailable (HTTP ${response.code})" }
                val body = checkNotNull(response.body) { "Image response is empty" }
                require(body.contentLength() <= 5 * 1024 * 1024) { "Image response is too large" }
                val bytes = body.byteStream().readNBytes(5 * 1024 * 1024 + 1)
                require(bytes.size <= 5 * 1024 * 1024) { "Image response is too large" }
                bytes
            }
        }
    }

    /** Calls the unauthenticated `/health` endpoint. */
    suspend fun health(): Result<HealthInfo> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/health").build()
            val body = execute(req).getOrThrow()
            json.decodeFromString<HealthInfo>(body)
        }
    }

    /** Retrieves gateway runtime status and provider metadata. */
    suspend fun status(): Result<GatewayStatus> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/status").addAuth().build()
            val body = execute(req).getOrThrow()
            json.decodeFromString<GatewayStatus>(body)
        }
    }

    /** Lists persisted sessions known to the gateway. */
    suspend fun sessions(): Result<List<SessionSummary>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/sessions").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val arr = element["sessions"]?.jsonArray ?: return@runCatching emptyList()
            arr.map { json.decodeFromJsonElement(SessionSummary.serializer(), it) }
        }
    }

    /** Loads stored message history for one session. */
    suspend fun sessionMessages(sessionId: String): Result<List<SessionMessage>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/sessions/$sessionId/messages").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val arr = element["messages"]?.jsonArray ?: return@runCatching emptyList()
            arr.map { json.decodeFromJsonElement(SessionMessage.serializer(), it) }
        }
    }

    /** Renames an existing session. */
    suspend fun renameSession(sessionId: String, name: String): Result<Unit> {
        val body = buildString {
            append("{\"name\":")
            append(kotlinx.serialization.json.Json.encodeToString(kotlinx.serialization.serializer<String>(), name))
            append("}")
        }
        val req = Request.Builder()
            .url("$baseUrl/api/sessions/$sessionId")
            .addAuth()
            .put(body.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Permanently deletes a session from the gateway. */
    suspend fun deleteSession(sessionId: String): Result<Unit> {
        val req = Request.Builder().url("$baseUrl/api/sessions/$sessionId").addAuth().delete().build()
        return execute(req).map {}
    }

    /** Requests abortion of the current generation for a session. */
    suspend fun abortSession(sessionId: String): Result<Unit> {
        val req = Request.Builder()
            .url("$baseUrl/api/sessions/$sessionId/abort")
            .addAuth()
            .post("{}".toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Retrieves the current gateway TOML configuration as plain text. */
    suspend fun config(): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/config").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            element["content"]?.jsonPrimitive?.content ?: ""
        }
    }

    /** Replaces the gateway TOML configuration with [toml]. */
    suspend fun updateConfig(toml: String): Result<Unit> {
        val req = Request.Builder()
            .url("$baseUrl/api/config")
            .addAuth()
            .put(toml.toRequestBody(TOML_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Retrieves the authenticated user's structured profile. */
    suspend fun userProfile(): Result<UserProfile> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/users/me/profile").addAuth().build()
            val body = execute(req).getOrThrow()
            json.decodeFromString<UserProfile>(body)
        }
    }

    /** Creates or replaces one explicit profile key. */
    suspend fun upsertUserProfileItem(input: UserProfileUpsert): Result<UserProfileItem> {
        val payload = json.encodeToString(UserProfileUpsert.serializer(), input)
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile")
            .addAuth()
            .post(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileItem>(it) }
    }

    /** Updates one profile item, including rejecting an inferred value. */
    suspend fun patchUserProfileItem(
        itemId: String,
        patch: UserProfilePatch,
    ): Result<UserProfileItem> {
        val encodedId = java.net.URLEncoder.encode(itemId, "UTF-8")
        val payload = json.encodeToString(UserProfilePatch.serializer(), patch)
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/items/$encodedId")
            .addAuth()
            .patch(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileItem>(it) }
    }

    /** Permanently deletes one profile item. */
    suspend fun deleteUserProfileItem(itemId: String): Result<Unit> {
        val encodedId = java.net.URLEncoder.encode(itemId, "UTF-8")
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/items/$encodedId")
            .addAuth()
            .delete()
            .build()
        return execute(req).map {}
    }

    /** Permanently deletes all profile items for the authenticated user. */
    suspend fun clearUserProfile(): Result<Int> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("$baseUrl/api/users/me/profile")
                .addAuth()
                .delete()
                .build()
            val body = execute(req).getOrThrow()
            json.parseToJsonElement(body).jsonObject["deleted"]?.jsonPrimitive?.content?.toInt()
                ?: 0
        }
    }

    /** Atomically imports a profile backup using the requested conflict strategy. */
    suspend fun importUserProfile(
        items: List<UserProfileImportItem>,
        strategy: ProfileImportStrategy,
    ): Result<UserProfileImportResult> {
        val payload = json.encodeToString(
            UserProfileImportRequest.serializer(),
            UserProfileImportRequest(strategy = strategy, items = items),
        )
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/import")
            .addAuth()
            .put(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileImportResult>(it) }
    }

    suspend fun searchUserProfile(request: UserProfileSearchRequest): Result<List<UserProfileItem>> {
        val payload = json.encodeToString(UserProfileSearchRequest.serializer(), request)
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/search")
            .addAuth()
            .post(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).mapCatching {
            json.decodeFromString<UserProfileSearchResult>(it).items
        }
    }

    suspend fun createUserProfileChangePlan(
        request: UserProfileChangePlanRequest,
    ): Result<UserProfileChangePlan> {
        val payload = json.encodeToString(UserProfileChangePlanRequest.serializer(), request)
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/change-plans")
            .addAuth()
            .post(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileChangePlan>(it) }
    }

    suspend fun applyUserProfileChangePlan(planId: String): Result<UserProfileMutationResult> {
        val encoded = java.net.URLEncoder.encode(planId, "UTF-8")
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/change-plans/$encoded/apply")
            .addAuth()
            .post(ByteArray(0).toRequestBody(null))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileMutationResult>(it) }
    }

    suspend fun undoUserProfileOperation(operationId: String): Result<UserProfileMutationResult> {
        val encoded = java.net.URLEncoder.encode(operationId, "UTF-8")
        val req = Request.Builder()
            .url("$baseUrl/api/users/me/profile/operations/$encoded/undo")
            .addAuth()
            .post(ByteArray(0).toRequestBody(null))
            .build()
        return execute(req).mapCatching { json.decodeFromString<UserProfileMutationResult>(it) }
    }

    suspend fun memories(
        query: String? = null,
        category: String? = null,
        namespace: String? = null,
        persona: String? = null,
    ): Result<MemoryEntries> = withContext(Dispatchers.IO) {
        runCatching {
            val url = baseUrl.toHttpUrl().newBuilder()
                .addPathSegments("api/memory")
                .apply {
                    query?.takeIf(String::isNotBlank)?.let { addQueryParameter("query", it) }
                    category?.let { addQueryParameter("category", it) }
                    namespace?.let { addQueryParameter("namespace", it) }
                    persona?.let { addQueryParameter("persona", it) }
                }
                .build()
            val body = execute(Request.Builder().url(url).addAuth().build()).getOrThrow()
            json.decodeFromString<MemoryEntries>(body)
        }
    }

    /** Retrieves personality files (SOUL.md, etc.) from the gateway. */
    suspend fun personality(): Result<Map<String, String>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/personality").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val filesObj = element["files"]?.jsonObject ?: return@runCatching emptyMap()
            filesObj.mapValues { it.value.jsonPrimitive.content }
        }
    }

    /** Lists named personas (分身) configured on the gateway. */
    suspend fun personas(): Result<List<PersonaInfo>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/personas").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val arr = element["personas"]?.jsonArray ?: return@runCatching emptyList()
            json.decodeFromJsonElement(
                kotlinx.serialization.builtins.ListSerializer(PersonaInfo.serializer()),
                arr,
            )
        }
    }

    /** Retrieves full detail for one named persona. */
    suspend fun persona(name: String): Result<PersonaDetail> = withContext(Dispatchers.IO) {
        runCatching {
            val encoded = java.net.URLEncoder.encode(name, "UTF-8")
            val req = Request.Builder().url("$baseUrl/api/personas/$encoded").addAuth().build()
            val body = execute(req).getOrThrow()
            json.decodeFromJsonElement(PersonaDetail.serializer(), json.parseToJsonElement(body).jsonObject)
        }
    }

    /** Creates or replaces one named persona's override fields. */
    suspend fun upsertPersona(name: String, persona: PersonaUpsert): Result<Unit> {
        val encoded = java.net.URLEncoder.encode(name, "UTF-8")
        val payload = json.encodeToString(PersonaUpsert.serializer(), persona)
        val req = Request.Builder()
            .url("$baseUrl/api/personas/$encoded")
            .addAuth()
            .put(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Deletes a named persona entry. */
    suspend fun deletePersona(name: String): Result<Unit> {
        val encoded = java.net.URLEncoder.encode(name, "UTF-8")
        val req = Request.Builder().url("$baseUrl/api/personas/$encoded").addAuth().delete().build()
        return execute(req).map {}
    }

    /** Updates personality files on the gateway. */
    suspend fun updatePersonality(files: Map<String, String>): Result<Unit> {
        val jsonBuilder = buildString {
            append("{\"files\":{")
            files.entries.forEachIndexed { i, (k, v) ->
                if (i > 0) append(",")
                append(kotlinx.serialization.json.Json.encodeToString(kotlinx.serialization.serializer<String>(), k))
                append(":")
                append(kotlinx.serialization.json.Json.encodeToString(kotlinx.serialization.serializer<String>(), v))
            }
            append("}}")
        }
        val req = Request.Builder()
            .url("$baseUrl/api/personality")
            .addAuth()
            .put(jsonBuilder.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Lists the tools currently available in the gateway runtime. */
    suspend fun tools(): Result<List<ToolInfo>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/tools").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val arr = element["tools"]?.jsonArray ?: return@runCatching emptyList()
            arr.map { json.decodeFromJsonElement(ToolInfo.serializer(), it) }
        }
    }

    /** Lists the skills currently available in the gateway runtime. */
    suspend fun skills(): Result<List<SkillInfo>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/skills").addAuth().build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            val arr = element["skills"]?.jsonArray ?: return@runCatching emptyList()
            arr.map { json.decodeFromJsonElement(SkillInfo.serializer(), it) }
        }
    }

    /** Reloads the skill index from disk and returns the new count. */
    suspend fun reloadSkills(): Result<Int> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("$baseUrl/api/skills/reload")
                .addAuth()
                .post("{}".toRequestBody(JSON_MEDIA_TYPE))
                .build()
            val body = execute(req).getOrThrow()
            val element = json.parseToJsonElement(body).jsonObject
            element["skills_count"]?.jsonPrimitive?.content?.toInt() ?: 0
        }
    }

    /** Retrieves full skill detail (including SKILL.md content) by name. */
    suspend fun skill(name: String): Result<SkillDetail> = withContext(Dispatchers.IO) {
        runCatching {
            val encoded = java.net.URLEncoder.encode(name, "UTF-8")
            val req = Request.Builder().url("$baseUrl/api/skills/$encoded").addAuth().build()
            val body = execute(req).getOrThrow()
            json.decodeFromJsonElement(SkillDetail.serializer(), json.parseToJsonElement(body).jsonObject)
        }
    }

    /** Updates the SKILL.md content for a skill by name. */
    suspend fun updateSkill(name: String, content: String): Result<Unit> {
        val encoded = java.net.URLEncoder.encode(name, "UTF-8")
        val payload = json.encodeToString(
            kotlinx.serialization.serializer<Map<String, String>>(),
            mapOf("content" to content),
        )
        val req = Request.Builder()
            .url("$baseUrl/api/skills/$encoded")
            .addAuth()
            .put(payload.toRequestBody(JSON_MEDIA_TYPE))
            .build()
        return execute(req).map {}
    }

    /** Lists models through the gateway provider proxy endpoint. */
    suspend fun models(provider: String? = null): Result<List<String>> = withContext(Dispatchers.IO) {
        runCatching {
            val url = "$baseUrl/api/provider/models".toHttpUrl().newBuilder().apply {
                provider?.takeIf { it.isNotBlank() }?.let { addQueryParameter("provider", it) }
            }.build()
            val req = Request.Builder().url(url).addAuth().build()
            val longClient = client.newBuilder()
                .connectTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
                .readTimeout(15, java.util.concurrent.TimeUnit.SECONDS)
                .build()
            val resp = longClient.newCall(req).execute()
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw Exception("HTTP ${resp.code}: ${body.take(200)}")
            parseModelsResponse(body)
        }
    }

    /** Lists all configured provider profiles for persona-level routing. */
    suspend fun providers(): Result<List<ProviderInfo>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/providers").addAuth().build()
            val body = execute(req).getOrThrow()
            val root = json.parseToJsonElement(body).jsonObject
            json.decodeFromJsonElement(
                ListSerializer(ProviderInfo.serializer()),
                root["providers"]?.jsonArray ?: kotlinx.serialization.json.JsonArray(emptyList()),
            )
        }
    }

    /** Query optional balances for a draft URL/key, or reuse its saved key when null. */
    suspend fun providerBalance(
        providerBaseUrl: String,
        apiKey: String?,
    ): Result<dev.clawseed.sdk.core.model.ProviderBalance> = withContext(Dispatchers.IO) {
        runCatching {
            val payload = kotlinx.serialization.json.buildJsonObject {
                put("base_url", kotlinx.serialization.json.JsonPrimitive(providerBaseUrl))
                apiKey?.let { put("api_key", kotlinx.serialization.json.JsonPrimitive(it)) }
            }
            val request = Request.Builder().url("$baseUrl/api/provider/balance")
                .addAuth().post(payload.toString().toRequestBody(JSON_MEDIA_TYPE)).build()
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
                json.decodeFromString<dev.clawseed.sdk.core.model.ProviderBalance>(response.body?.string().orEmpty())
            }
        }
    }

    /** Lists models directly from a provider-compatible `/models` endpoint. */
    suspend fun fetchProviderModels(
        providerBaseUrl: String,
        apiKey: String,
    ): Result<List<String>> = withContext(Dispatchers.IO) {
        runCatching {
            val url = providerBaseUrl.trimEnd('/') + "/models"
            val reqBuilder = Request.Builder().url(url)
            if (apiKey.isNotBlank()) {
                reqBuilder.addHeader("Authorization", "Bearer $apiKey")
            }
            val longClient = client.newBuilder()
                .connectTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
                .readTimeout(15, java.util.concurrent.TimeUnit.SECONDS)
                .build()
            val resp = longClient.newCall(reqBuilder.build()).execute()
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw Exception("HTTP ${resp.code}: ${body.take(200)}")
            parseModelsResponse(body)
        }
    }

    /** Sends a message to the `/webhook` synchronous execution endpoint. */
    suspend fun webhook(
        message: String,
        sessionId: String? = null,
    ): Result<WebhookResponse> = withContext(Dispatchers.IO) {
        runCatching {
            val body = """{"message":${kotlinx.serialization.json.Json.encodeToString(kotlinx.serialization.serializer<String>(), message)}}"""
            val reqBuilder = Request.Builder()
                .url("$baseUrl/webhook")
                .addAuth()
                .post(body.toRequestBody(JSON_MEDIA_TYPE))
            sessionId?.let { reqBuilder.addHeader("X-Session-Id", it) }
            val longClient = client.newBuilder()
                .connectTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
                .readTimeout(300, java.util.concurrent.TimeUnit.SECONDS)
                .build()
            val resp = longClient.newCall(reqBuilder.build()).execute()
            val respBody = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw Exception("HTTP ${resp.code}: ${respBody.take(200)}")
            json.decodeFromString<WebhookResponse>(respBody)
        }
    }

    private fun parseModelsResponse(body: String): List<String> {
        val element = json.parseToJsonElement(body).jsonObject
        val data = element["data"]?.jsonArray ?: throw Exception("Missing data field")
        return data.mapNotNull { it.jsonObject["id"]?.jsonPrimitive?.content }
            .filter { it.isNotBlank() }
            .sorted()
    }

    companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private val TOML_MEDIA_TYPE = "text/plain; charset=utf-8".toMediaType()

        /** Creates the default HTTP client used by [GatewayClient]. */
        fun defaultHttpClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(5, java.util.concurrent.TimeUnit.SECONDS)
            .readTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
            .build()
    }
}
