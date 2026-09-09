package dev.clawseed.sdk.core.model

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Versioned client-facing content sent with a tool result. */
data class ToolPresentation(
    val version: Int,
    val blocks: List<ContentBlock>,
)

/** Structured content that can be rendered without scraping model text. */
sealed interface ContentBlock {
    data class Markdown(val text: String) : ContentBlock
    data class Image(val media: MediaReference, val alt: String?) : ContentBlock
    data class Audio(val media: MediaReference, val title: String?) : ContentBlock
    data class Video(val media: MediaReference, val title: String?) : ContentBlock
    data class Link(
        val title: String,
        val url: String,
        val description: String?,
        val source: String?,
        val thumbnail: MediaReference?,
    ) : ContentBlock
    data class SearchResults(val query: String, val items: List<SearchResultItem>) : ContentBlock
    data class Profile(
        val title: String,
        val summary: String,
        val planId: String?,
        val operationId: String?,
        val profileVersion: Long,
        val requiresConfirmation: Boolean,
        val items: List<ProfilePresentationItem>,
        val actions: List<PresentationAction>,
    ) : ContentBlock

    /** Preserves forward-compatible blocks that this SDK version cannot render yet. */
    data class Unsupported(val type: String, val raw: JsonObject) : ContentBlock
}

data class MediaReference(
    val assetId: String?,
    val url: String?,
    val mimeType: String?,
    val thumbnailUrl: String?,
    val width: Int?,
    val height: Int?,
    val durationMs: Long?,
)

data class SearchResultItem(
    val id: String?,
    val title: String,
    val url: String,
    val description: String?,
    val source: String?,
    val thumbnail: MediaReference?,
)

data class ProfilePresentationItem(
    val id: String,
    val key: String,
    val before: JsonElement?,
    val after: JsonElement?,
    val source: String?,
    val status: String?,
)

data class PresentationAction(
    val id: String,
    val label: String,
    val command: String,
    val destructive: Boolean,
)

/** Parses a gateway presentation while preserving unsupported block types. */
fun parseToolPresentation(element: JsonElement?): ToolPresentation? {
    val root = element as? JsonObject ?: return null
    val version = root["version"]?.jsonPrimitive?.intOrNull ?: return null
    val blocks = root["blocks"]?.jsonArray?.mapNotNull { it as? JsonObject }?.map(::parseContentBlock)
        ?: return null
    return ToolPresentation(version = version, blocks = blocks)
}

private fun parseContentBlock(block: JsonObject): ContentBlock {
    return when (val type = block.string("type") ?: "unknown") {
        "markdown" -> ContentBlock.Markdown(block.string("text").orEmpty())
        "image" -> ContentBlock.Image(parseMedia(block["media"]), block.string("alt"))
        "audio" -> ContentBlock.Audio(parseMedia(block["media"]), block.string("title"))
        "video" -> ContentBlock.Video(parseMedia(block["media"]), block.string("title"))
        "link" -> ContentBlock.Link(
            title = block.string("title").orEmpty(),
            url = block.string("url").orEmpty(),
            description = block.string("description"),
            source = block.string("source"),
            thumbnail = block["thumbnail"]?.let(::parseMedia),
        )
        "search_results" -> ContentBlock.SearchResults(
            query = block.string("query").orEmpty(),
            items = block["items"]?.jsonArray.orEmpty().mapNotNull { it as? JsonObject }.map { item ->
                SearchResultItem(
                    id = item.string("id"),
                    title = item.string("title").orEmpty(),
                    url = item.string("url").orEmpty(),
                    description = item.string("description"),
                    source = item.string("source"),
                    thumbnail = item["thumbnail"]?.let(::parseMedia),
                )
            },
        )
        "profile" -> ContentBlock.Profile(
            title = block.string("title").orEmpty(),
            summary = block.string("summary").orEmpty(),
            planId = block.string("plan_id"),
            operationId = block.string("operation_id"),
            profileVersion = block["profile_version"]?.jsonPrimitive?.longOrNull ?: 0,
            requiresConfirmation = block["requires_confirmation"]?.jsonPrimitive?.booleanOrNull ?: false,
            items = block["items"]?.jsonArray.orEmpty().mapNotNull { it as? JsonObject }.map { item ->
                ProfilePresentationItem(
                    id = item.string("id").orEmpty(),
                    key = item.string("key").orEmpty(),
                    before = item["before"],
                    after = item["after"],
                    source = item.string("source"),
                    status = item.string("status"),
                )
            },
            actions = block["actions"]?.jsonArray.orEmpty().mapNotNull { it as? JsonObject }.map { action ->
                PresentationAction(
                    id = action.string("id").orEmpty(),
                    label = action.string("label").orEmpty(),
                    command = action.string("command").orEmpty(),
                    destructive = action["destructive"]?.jsonPrimitive?.booleanOrNull ?: false,
                )
            },
        )
        else -> ContentBlock.Unsupported(type, block)
    }
}

private fun parseMedia(element: JsonElement?): MediaReference {
    val media = element as? JsonObject
    return MediaReference(
        assetId = media?.string("asset_id"),
        url = media?.string("url"),
        mimeType = media?.string("mime_type"),
        thumbnailUrl = media?.string("thumbnail_url"),
        width = media?.get("width")?.jsonPrimitive?.intOrNull,
        height = media?.get("height")?.jsonPrimitive?.intOrNull,
        durationMs = media?.get("duration_ms")?.jsonPrimitive?.contentOrNull?.toLongOrNull(),
    )
}

private fun JsonObject.string(name: String): String? = this[name]?.jsonPrimitive?.contentOrNull
