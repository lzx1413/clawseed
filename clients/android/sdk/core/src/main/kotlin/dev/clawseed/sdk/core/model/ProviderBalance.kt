package dev.clawseed.sdk.core.model

import kotlinx.serialization.Serializable

@Serializable
data class BalanceAmount(val currency: String, val available: String)

/** Optional account balance; status is separate from a known zero balance. */
@Serializable
data class ProviderBalance(
    val status: String,
    val balances: List<BalanceAmount> = emptyList(),
)
