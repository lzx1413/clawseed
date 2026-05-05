package dev.clawseed.sdk.android.cetp

data class DiscoveredProvider(
    val packageName: String,
    val authority: String,
    val version: Int,
    val providerLabel: String,
    val tools: List<DiscoveredTool>,
    val providerInfo: ProviderInfo?,
)

data class DiscoveredTool(
    val name: String,
    val namespacedName: String,
    val description: String,
    val parametersJson: String,
)

data class ProviderInfo(
    val providerName: String,
    val description: String,
    val scopes: List<ProviderScope>,
)

data class ProviderScope(
    val name: String,
    val description: String,
)

data class AuthRequiredEvent(
    val providerPackageName: String,
    val providerLabel: String,
    val toolName: String,
    val resolutionHint: String?,
    val authorizeIntent: String?,
)
