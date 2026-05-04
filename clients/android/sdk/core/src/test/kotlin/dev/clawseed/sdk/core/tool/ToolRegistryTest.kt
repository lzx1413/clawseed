package dev.clawseed.sdk.core.tool

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.test.Test
import kotlin.test.assertEquals

class ToolRegistryTest {

    @Test
    fun lambdaRegistrationNotifiesListeners() {
        val registry = ToolRegistry()
        var notifications = 0
        registry.onToolRegistered { notifications += 1 }

        registry.register(
            name = "device_info",
            description = "Get device information",
            parameters = """{"type":"object","properties":{}}""",
        ) { ToolResult.Success("ok") }

        assertEquals(1, notifications)
        assertEquals(listOf("device_info"), registry.registeredTools().map { it.name })
    }

    @Test
    fun interfaceRegistrationNotifiesListeners() {
        val registry = ToolRegistry()
        var notifications = 0
        registry.onToolRegistered { notifications += 1 }

        registry.register(object : ClawSeedTool {
            override val name: String = "get_location"
            override val description: String = "Get current location"
            override val parametersSchema = buildJsonObject {
                put("type", "object")
            }

            override suspend fun execute(args: kotlinx.serialization.json.JsonObject): ToolResult {
                return ToolResult.Success("ok")
            }
        })

        assertEquals(1, notifications)
        assertEquals(listOf("get_location"), registry.registeredTools().map { it.name })
    }
}