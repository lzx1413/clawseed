package dev.clawseed.demo.data

import com.google.gson.Gson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject

class GatewayApi(
    private val baseUrl: String = "http://127.0.0.1:42617",
    private val bearerToken: () -> String? = { null },
) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(5, java.util.concurrent.TimeUnit.SECONDS)
        .readTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
        .build()
    private val gson = Gson()
    private val jsonType = "application/json; charset=utf-8".toMediaType()

    private fun Request.Builder.addAuth(): Request.Builder {
        bearerToken()?.let { addHeader("Authorization", "Bearer $it") }
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

    suspend fun getSessions(): Result<List<ChatSession>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/sessions").addAuth().build()
            val body = execute(req).getOrThrow()
            val wrapper = gson.fromJson(body, SessionsResponse::class.java)
            wrapper.sessions.map { it.toChatSession() }
        }
    }

    suspend fun getSessionMessages(sessionId: String): Result<List<SessionMessage>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/sessions/$sessionId/messages").addAuth().build()
            val body = execute(req).getOrThrow()
            val wrapper = gson.fromJson(body, MessagesResponse::class.java)
            wrapper.messages
        }
    }

    suspend fun renameSession(sessionId: String, name: String): Result<Unit> {
        val json = JSONObject().put("name", name).toString()
        val req = Request.Builder()
            .url("$baseUrl/api/sessions/$sessionId")
            .addAuth()
            .put(json.toRequestBody(jsonType))
            .build()
        return execute(req).map {}
    }

    suspend fun deleteSession(sessionId: String): Result<Unit> {
        val req = Request.Builder().url("$baseUrl/api/sessions/$sessionId").addAuth().delete().build()
        return execute(req).map {}
    }

    suspend fun getConfig(): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/config").addAuth().build()
            val body = execute(req).getOrThrow()
            val json = gson.fromJson(body, ConfigResponse::class.java)
            json.content
        }
    }

    suspend fun putConfig(toml: String): Result<Unit> {
        val req = Request.Builder()
            .url("$baseUrl/api/config")
            .addAuth()
            .put(toml.toRequestBody("text/plain; charset=utf-8".toMediaType()))
            .build()
        return execute(req).map {}
    }

    suspend fun getTools(): Result<List<ToolInfo>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/tools").addAuth().build()
            val body = execute(req).getOrThrow()
            val wrapper = gson.fromJson(body, ToolsResponse::class.java)
            wrapper.tools
        }
    }

    suspend fun getStatus(): Result<StatusInfo> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder().url("$baseUrl/api/status").addAuth().build()
            val body = execute(req).getOrThrow()
            gson.fromJson(body, StatusInfo::class.java)
        }
    }

    suspend fun abortSession(sessionId: String): Result<Unit> {
        val req = Request.Builder()
            .url("$baseUrl/api/sessions/$sessionId/abort")
            .addAuth()
            .post("{}".toRequestBody(jsonType))
            .build()
        return execute(req).map {}
    }

    suspend fun fetchModels(providerBaseUrl: String, apiKey: String): Result<List<String>> = withContext(Dispatchers.IO) {
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

    suspend fun fetchModelsViaGateway(): Result<List<String>> = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("$baseUrl/api/provider/models")
                .addAuth()
                .build()
            val longClient = client.newBuilder()
                .connectTimeout(10, java.util.concurrent.TimeUnit.SECONDS)
                .readTimeout(15, java.util.concurrent.TimeUnit.SECONDS)
                .build()
            val resp = longClient.newCall(req).execute()
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) {
                val error = try { JSONObject(body).optString("error", body.take(200)) } catch (_: Exception) { body.take(200) }
                throw Exception(error)
            }
            parseModelsResponse(body)
        }
    }

    private fun parseModelsResponse(body: String): List<String> {
        val json = JSONObject(body)
        val data = json.optJSONArray("data") ?: throw Exception("响应格式错误: 缺少 data 字段")
        val models = mutableListOf<String>()
        for (i in 0 until data.length()) {
            val obj = data.getJSONObject(i)
            val id = obj.optString("id", "")
            if (id.isNotBlank()) models.add(id)
        }
        return models.sorted()
    }
}

data class SessionMessage(
    val role: String,
    val content: String?,
    val tool_name: String? = null,
    val tool_args: String? = null,
    val tool_result: String? = null,
    val success: Boolean? = null,
)

// Gateway returns {"sessions": [...]} wrapper
private data class SessionsResponse(
    val sessions: List<GatewaySession> = emptyList(),
)

// Gateway returns {"messages": [...], "session_id": "...", "session_persistence": true}
private data class MessagesResponse(
    val messages: List<SessionMessage> = emptyList(),
)

private data class GatewaySession(
    val session_id: String,
    val name: String? = null,
    val created_at: String = "",
    val last_activity: String = "",
    val message_count: Int = 0,
) {
    fun toChatSession() = ChatSession(
        id = session_id,
        name = name,
        createdAt = parseEpochMillis(created_at),
        lastActivity = parseEpochMillis(last_activity),
        messageCount = message_count,
    )
}

private fun parseEpochMillis(iso: String): Long {
    return try {
        java.time.Instant.parse(iso).toEpochMilli()
    } catch (_: Exception) {
        0L
    }
}

private data class ConfigResponse(
    val content: String = "",
    val format: String = "toml",
)

private data class ToolsResponse(
    val tools: List<ToolInfo> = emptyList(),
)
