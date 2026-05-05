package dev.clawseed.sdk.android.cetp

import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import dev.clawseed.sdk.core.tool.ClawSeedTool
import dev.clawseed.sdk.core.tool.ToolRegistry
import dev.clawseed.sdk.core.tool.ToolResult
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ExternalToolBridge(private val context: Context) {

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val cetpClient = CetpClient(context)
    private val json = Json { ignoreUnknownKeys = true }

    private val _providers = MutableStateFlow<List<DiscoveredProvider>>(emptyList())
    val providers: StateFlow<List<DiscoveredProvider>> = _providers.asStateFlow()

    private val _authEvents = MutableSharedFlow<AuthRequiredEvent>(extraBufferCapacity = 8)
    val authEvents: SharedFlow<AuthRequiredEvent> = _authEvents.asSharedFlow()

    private var currentRegistry: ToolRegistry? = null
    private var packageChangeReceiver: PackageChangeReceiver? = null
    private var registeredToolNames = mutableSetOf<String>()
    private var scanned = false

    fun attachToRegistry(registry: ToolRegistry) {
        if (currentRegistry == registry) return
        detachFromRegistry()
        currentRegistry = registry
        scope.launch {
            // Always rescan on attach — provider list may have changed
            // since last session (app installed/updated/uninstalled while
            // we were disconnected, or tool list changed server-side).
            rescan()
        }
    }

    fun detachFromRegistry() {
        val registry = currentRegistry ?: return
        for (name in registeredToolNames) {
            registry.unregister(name)
        }
        registeredToolNames.clear()
        currentRegistry = null
    }

    suspend fun scan() {
        val discovered = mutableListOf<DiscoveredProvider>()
        val intent = android.content.Intent(CetpConstants.ACTION_TOOL_PROVIDER)
        val resolveInfos = context.packageManager.queryIntentServices(
            intent,
            PackageManager.GET_META_DATA,
        )

        Log.d(TAG, "scan: found ${resolveInfos.size} provider service(s)")

        val usedLabels = mutableSetOf<String>()

        for (info in resolveInfos) {
            val serviceInfo = info.serviceInfo ?: continue
            val metaData = serviceInfo.metaData ?: continue
            val authority = metaData.getString(CetpConstants.META_AUTHORITY) ?: continue
            val version = metaData.getInt(CetpConstants.META_VERSION, 1)
            if (version > CetpConstants.SUPPORTED_PROTOCOL_VERSION) continue

            val packageName = serviceInfo.packageName
            val label = deriveProviderLabel(packageName, usedLabels)
            usedLabels.add(label)

            Log.d(TAG, "scan: querying $packageName authority=$authority label=$label")

            val toolsResult = cetpClient.listTools(authority)
            if (toolsResult !is CetpResult.Success) {
                Log.w(TAG, "scan: list_tools failed for $authority: $toolsResult")
                // Provider process may not be running. Try to start it via
                // an explicit intent to the discovery service, then retry.
                try {
                    val serviceIntent = android.content.Intent(CetpConstants.ACTION_TOOL_PROVIDER).apply {
                        setPackage(packageName)
                    }
                    // Start the provider app's main activity as a fallback to
                    // ensure its process is running.
                    val launchIntent = context.packageManager.getLaunchIntentForPackage(packageName)
                    if (launchIntent != null) {
                        launchIntent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                        context.startActivity(launchIntent)
                        Thread.sleep(500)
                    }
                } catch (_: Exception) {}
                val retryResult = cetpClient.listTools(authority)
                if (retryResult !is CetpResult.Success) {
                    Log.w(TAG, "scan: list_tools retry also failed for $authority")
                    continue
                }
                val retryTools = parseTools(retryResult.data, label)
                if (retryTools.isEmpty()) continue

                Log.d(TAG, "scan: $label provides ${retryTools.size} tool(s) (after process start)")

                val providerInfo = parseProviderInfo(authority)
                discovered.add(
                    DiscoveredProvider(
                        packageName = packageName,
                        authority = authority,
                        version = version,
                        providerLabel = label,
                        tools = retryTools,
                        providerInfo = providerInfo,
                    ),
                )
                continue
            }

            val tools = parseTools(toolsResult.data, label)
            if (tools.isEmpty()) continue

            Log.d(TAG, "scan: $label provides ${tools.size} tool(s)")

            val providerInfo = parseProviderInfo(authority)

            discovered.add(
                DiscoveredProvider(
                    packageName = packageName,
                    authority = authority,
                    version = version,
                    providerLabel = label,
                    tools = tools,
                    providerInfo = providerInfo,
                ),
            )
        }

        _providers.value = discovered
        scanned = true

        Log.d(TAG, "scan: ${discovered.size} provider(s), ${discovered.sumOf { it.tools.size }} tool(s) total")

        val registry = currentRegistry
        if (registry != null) {
            registerAllTools(registry)
        }
    }

    suspend fun rescan() {
        val registry = currentRegistry
        if (registry != null) {
            for (name in registeredToolNames) {
                registry.unregister(name)
            }
            registeredToolNames.clear()
        }
        scanned = false
        scan()
    }

    fun startWatching() {
        if (packageChangeReceiver != null) return
        val receiver = PackageChangeReceiver { action, packageName ->
            // Filter: only rescan if the changed package could be a CETP provider.
            // Check if it declares the TOOL_PROVIDER intent; skip irrelevant packages.
            scope.launch {
                if (isPotentialProvider(packageName)) {
                    rescan()
                }
            }
        }
        context.registerReceiver(receiver, PackageChangeReceiver.createFilter())
        packageChangeReceiver = receiver
    }

    fun stopWatching() {
        val receiver = packageChangeReceiver ?: return
        try {
            context.unregisterReceiver(receiver)
        } catch (_: Exception) {
        }
        packageChangeReceiver = null
    }

    private fun registerAllTools(registry: ToolRegistry) {
        for (provider in _providers.value) {
            for (tool in provider.tools) {
                val proxyTool = CetpProxyTool(
                    name = tool.namespacedName,
                    description = tool.description,
                    parametersSchema = json.parseToJsonElement(tool.parametersJson).jsonObject,
                    authority = provider.authority,
                    localToolName = tool.name,
                    providerPackageName = provider.packageName,
                    providerLabel = provider.providerLabel,
                )
                registry.register(proxyTool)
                registeredToolNames.add(tool.namespacedName)
            }
        }
    }

    private fun parseTools(dataJson: String, label: String): List<DiscoveredTool> {
        return try {
            val root = json.parseToJsonElement(dataJson).jsonObject
            val toolsArray = root["tools"]?.jsonArray ?: return emptyList()
            toolsArray.mapNotNull { element ->
                val obj = element.jsonObject
                val name = obj["name"]?.jsonPrimitive?.content ?: return@mapNotNull null
                val description = obj["description"]?.jsonPrimitive?.content ?: ""
                val parameters = obj["parameters"]?.toString() ?: "{}"
                DiscoveredTool(
                    name = name,
                    namespacedName = "${label}${CetpConstants.NAMESPACE_SEPARATOR}${name}",
                    description = description,
                    parametersJson = parameters,
                )
            }
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun parseProviderInfo(authority: String): ProviderInfo? {
        val result = cetpClient.getProviderInfo(authority) ?: return null
        if (result !is CetpResult.Success) return null
        return try {
            val root = json.parseToJsonElement(result.data).jsonObject
            val scopesArray = root["scopes"]?.jsonArray ?: emptyList()
            val scopes = scopesArray.mapNotNull { element ->
                val obj = element.jsonObject
                val name = obj["name"]?.jsonPrimitive?.content ?: return@mapNotNull null
                val desc = obj["description"]?.jsonPrimitive?.content ?: ""
                ProviderScope(name, desc)
            }
            ProviderInfo(
                providerName = root["provider_name"]?.jsonPrimitive?.content ?: "",
                description = root["description"]?.jsonPrimitive?.content ?: "",
                scopes = scopes,
            )
        } catch (_: Exception) {
            null
        }
    }

    private fun deriveProviderLabel(packageName: String, usedLabels: Set<String>): String {
        val segments = packageName.split(".")
        val base = segments.lastOrNull()
            ?.replace(Regex("[^a-zA-Z0-9]"), "_")
            ?.lowercase()
            ?: "provider"
        if (base !in usedLabels) return base
        var i = 2
        while ("${base}_$i" in usedLabels) i++
        return "${base}_$i"
    }

    private fun isPotentialProvider(packageName: String): Boolean {
        // Already known provider — definitely need to rescan
        if (_providers.value.any { it.packageName == packageName }) return true
        // Check if the package declares TOOL_PROVIDER intent
        val intent = android.content.Intent(CetpConstants.ACTION_TOOL_PROVIDER).apply {
            setPackage(packageName)
        }
        return context.packageManager.queryIntentServices(intent, 0).isNotEmpty()
    }

    companion object {
        private const val TAG = "ExternalToolBridge"
    }

    private inner class CetpProxyTool(
        override val name: String,
        override val description: String,
        override val parametersSchema: JsonObject,
        private val authority: String,
        private val localToolName: String,
        private val providerPackageName: String,
        private val providerLabel: String,
    ) : ClawSeedTool {

        override suspend fun execute(args: JsonObject): ToolResult {
            val argsJson = args.toString()
            val requestId = java.util.UUID.randomUUID().toString()
            val result = cetpClient.executeTool(authority, localToolName, argsJson, requestId)

            return when (result) {
                is CetpResult.Success -> ToolResult.Success(result.data)
                is CetpResult.Error -> {
                    if (result.errorCode == CetpConstants.ERROR_AUTH_REQUIRED) {
                        scope.launch {
                            _authEvents.emit(
                                AuthRequiredEvent(
                                    providerPackageName = providerPackageName,
                                    providerLabel = providerLabel,
                                    toolName = name,
                                    resolutionHint = result.resolutionHint,
                                    authorizeIntent = result.authorizeIntent,
                                ),
                            )
                        }
                    }
                    val message = buildString {
                        append("[${result.errorCode}] ${result.errorMessage}")
                        if (result.resolutionHint != null) {
                            append(" — ${result.resolutionHint}")
                        }
                    }
                    ToolResult.Failure(message)
                }
            }
        }
    }
}
